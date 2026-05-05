use std::{fmt::Write, path::Path};

use anyhow::{Result, bail};
use axum::{
    Form,
    extract::{Multipart, Path as AxumPath, State},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse, Response},
};
use chrono::{DateTime, Datelike, Utc};
use serde::Deserialize;
use sqlx::Row;
use uuid::Uuid;

use crate::{
    AppState, auth,
    html::{escape_html, page_end, page_start, redirect, server_error_page},
};

#[derive(Debug)]
struct MediaAsset {
    id: Uuid,
    filename: String,
    original_filename: String,
    mime_type: String,
    size_bytes: i64,
    storage_path: String,
    width: Option<i32>,
    height: Option<i32>,
    alt_text: Option<String>,
    created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct MediaAltForm {
    #[serde(default)]
    csrf_token: String,
    #[serde(default)]
    alt_text: String,
}

struct PendingUpload {
    bytes: Vec<u8>,
    original_filename: String,
}

#[derive(Clone, Copy)]
struct ImageInfo {
    extension: &'static str,
    mime_type: &'static str,
    width: Option<i32>,
    height: Option<i32>,
}

pub async fn admin_list(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if auth::current_admin(&state, &headers).await.is_none() {
        return redirect("/admin/login");
    }

    match list_assets(&state).await {
        Ok(assets) => {
            media_html(&assets, None, &auth::csrf_input(&state, &headers)).into_response()
        }
        Err(error) => server_error(error),
    }
}

pub async fn admin_upload(
    State(state): State<AppState>,
    headers: HeaderMap,
    multipart: Multipart,
) -> Response {
    let Some(user) = auth::current_admin(&state, &headers).await else {
        return redirect("/admin/login");
    };

    let csrf_input = auth::csrf_input(&state, &headers);
    match store_upload(&state, &user, &headers, multipart).await {
        Ok(()) => redirect("/admin/media"),
        Err(error) => match list_assets(&state).await {
            Ok(assets) => (
                StatusCode::BAD_REQUEST,
                media_html(&assets, Some(&error.to_string()), &csrf_input),
            )
                .into_response(),
            Err(_) => (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
        },
    }
}

pub async fn admin_update(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(id): AxumPath<Uuid>,
    Form(form): Form<MediaAltForm>,
) -> Response {
    if auth::current_admin(&state, &headers).await.is_none() {
        return redirect("/admin/login");
    }
    if !auth::verify_csrf(&state, &headers, &form.csrf_token) {
        return auth::csrf_rejection();
    }

    match update_alt_text(&state, id, &form.alt_text).await {
        Ok(()) => redirect("/admin/media"),
        Err(error) => match list_assets(&state).await {
            Ok(assets) => (
                StatusCode::BAD_REQUEST,
                media_html(
                    &assets,
                    Some(&error.to_string()),
                    &auth::csrf_input(&state, &headers),
                ),
            )
                .into_response(),
            Err(_) => (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
        },
    }
}

async fn list_assets(state: &AppState) -> Result<Vec<MediaAsset>> {
    let rows = sqlx::query(
        r#"
        SELECT id, filename, original_filename, mime_type, size_bytes, storage_path, width, height, alt_text, created_at
        FROM media_assets
        ORDER BY created_at DESC
        LIMIT 60
        "#,
    )
    .fetch_all(&state.pool)
    .await?;

    rows.into_iter()
        .map(|row| {
            Ok(MediaAsset {
                id: row.try_get("id")?,
                filename: row.try_get("filename")?,
                original_filename: row.try_get("original_filename")?,
                mime_type: row.try_get("mime_type")?,
                size_bytes: row.try_get("size_bytes")?,
                storage_path: row.try_get("storage_path")?,
                width: row.try_get("width")?,
                height: row.try_get("height")?,
                alt_text: row.try_get("alt_text")?,
                created_at: row.try_get("created_at")?,
            })
        })
        .collect()
}

async fn update_alt_text(state: &AppState, id: Uuid, alt_text: &str) -> Result<()> {
    let alt_text = normalize_alt_text(alt_text)?;
    sqlx::query("UPDATE media_assets SET alt_text = $1, updated_at = now() WHERE id = $2")
        .bind(alt_text)
        .bind(id)
        .execute(&state.pool)
        .await?;
    Ok(())
}

async fn store_upload(
    state: &AppState,
    user: &auth::AdminUser,
    headers: &HeaderMap,
    mut multipart: Multipart,
) -> Result<()> {
    let mut upload = None;
    let mut alt_text = None;
    let mut csrf_token = None;

    while let Some(field) = multipart.next_field().await? {
        let Some(name) = field.name().map(str::to_owned) else {
            continue;
        };

        match name.as_str() {
            "file" if upload.is_none() => {
                let original_filename = field
                    .file_name()
                    .map(clean_original_filename)
                    .unwrap_or_else(|| "upload".to_owned());
                let bytes = field.bytes().await?;
                if bytes.is_empty() {
                    bail!("choose an image to upload");
                }
                if bytes.len() as u64 > state.config.max_upload_bytes {
                    bail!(
                        "image is larger than the configured {} byte limit",
                        state.config.max_upload_bytes
                    );
                }
                upload = Some(PendingUpload {
                    bytes: bytes.to_vec(),
                    original_filename,
                });
            }
            "alt_text" => {
                let text = field.text().await?;
                alt_text = normalize_alt_text(&text)?;
            }
            "csrf_token" => {
                csrf_token = Some(field.text().await?);
            }
            _ => {}
        }
    }

    if !auth::verify_csrf(state, headers, csrf_token.as_deref().unwrap_or("")) {
        bail!("invalid CSRF token");
    }

    let Some(upload) = upload else {
        bail!("choose an image to upload");
    };
    let Some(image) = sniff_image_kind(&upload.bytes) else {
        bail!("image must be a PNG, JPEG, GIF, or WebP file");
    };

    let now = Utc::now();
    let generated_filename = format!("{}.{}", Uuid::new_v4(), image.extension);
    let storage_path = format!(
        "{:04}/{:02}/{}",
        now.year(),
        now.month(),
        generated_filename
    );
    let directory = state
        .config
        .upload_dir
        .join(format!("{:04}", now.year()))
        .join(format!("{:02}", now.month()));
    tokio::fs::create_dir_all(&directory).await?;
    tokio::fs::write(directory.join(&generated_filename), &upload.bytes).await?;

    sqlx::query(
        r#"
        INSERT INTO media_assets (
            filename,
            original_filename,
            mime_type,
            size_bytes,
            storage_path,
            width,
            height,
            alt_text,
            uploaded_by
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        "#,
    )
    .bind(&generated_filename)
    .bind(&upload.original_filename)
    .bind(image.mime_type)
    .bind(upload.bytes.len() as i64)
    .bind(&storage_path)
    .bind(image.width)
    .bind(image.height)
    .bind(alt_text)
    .bind(user.id)
    .execute(&state.pool)
    .await?;

    Ok(())
}

fn clean_original_filename(value: &str) -> String {
    let filename = Path::new(value)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("upload");
    let cleaned: String = filename
        .chars()
        .filter(|ch| !ch.is_control())
        .take(255)
        .collect();

    if cleaned.trim().is_empty() {
        "upload".to_owned()
    } else {
        cleaned
    }
}

fn normalize_alt_text(value: &str) -> Result<Option<String>> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    if value.chars().count() > 255 {
        bail!("alt text must be 255 characters or fewer");
    }
    Ok(Some(value.to_owned()))
}

fn sniff_image_kind(bytes: &[u8]) -> Option<ImageInfo> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        let dimensions = png_dimensions(bytes);
        return Some(ImageInfo {
            extension: "png",
            mime_type: "image/png",
            width: dimensions.map(|(width, _)| width),
            height: dimensions.map(|(_, height)| height),
        });
    }
    if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        let dimensions = jpeg_dimensions(bytes);
        return Some(ImageInfo {
            extension: "jpg",
            mime_type: "image/jpeg",
            width: dimensions.map(|(width, _)| width),
            height: dimensions.map(|(_, height)| height),
        });
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        let dimensions = gif_dimensions(bytes);
        return Some(ImageInfo {
            extension: "gif",
            mime_type: "image/gif",
            width: dimensions.map(|(width, _)| width),
            height: dimensions.map(|(_, height)| height),
        });
    }
    if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        let dimensions = webp_dimensions(bytes);
        return Some(ImageInfo {
            extension: "webp",
            mime_type: "image/webp",
            width: dimensions.map(|(width, _)| width),
            height: dimensions.map(|(_, height)| height),
        });
    }
    None
}

fn png_dimensions(bytes: &[u8]) -> Option<(i32, i32)> {
    if bytes.len() < 24 {
        return None;
    }
    let width = u32::from_be_bytes(bytes[16..20].try_into().ok()?);
    let height = u32::from_be_bytes(bytes[20..24].try_into().ok()?);
    dimensions_to_i32(width, height)
}

fn gif_dimensions(bytes: &[u8]) -> Option<(i32, i32)> {
    if bytes.len() < 10 {
        return None;
    }
    let width = u16::from_le_bytes(bytes[6..8].try_into().ok()?);
    let height = u16::from_le_bytes(bytes[8..10].try_into().ok()?);
    dimensions_to_i32(u32::from(width), u32::from(height))
}

fn jpeg_dimensions(bytes: &[u8]) -> Option<(i32, i32)> {
    let mut index = 2;
    while index + 4 < bytes.len() {
        if bytes[index] != 0xff {
            index += 1;
            continue;
        }
        while index < bytes.len() && bytes[index] == 0xff {
            index += 1;
        }
        if index >= bytes.len() {
            return None;
        }

        let marker = bytes[index];
        index += 1;
        if marker == 0xd9 || marker == 0xda {
            return None;
        }
        if matches!(marker, 0x01 | 0xd0..=0xd7) {
            continue;
        }
        if index + 2 > bytes.len() {
            return None;
        }

        let segment_len = u16::from_be_bytes(bytes[index..index + 2].try_into().ok()?);
        let segment_len = usize::from(segment_len);
        if segment_len < 2 || index + segment_len > bytes.len() {
            return None;
        }

        if is_jpeg_sof(marker) && segment_len >= 7 {
            let data = index + 2;
            let height = u16::from_be_bytes(bytes[data + 1..data + 3].try_into().ok()?);
            let width = u16::from_be_bytes(bytes[data + 3..data + 5].try_into().ok()?);
            return dimensions_to_i32(u32::from(width), u32::from(height));
        }

        index += segment_len;
    }
    None
}

fn is_jpeg_sof(marker: u8) -> bool {
    matches!(
        marker,
        0xc0 | 0xc1 | 0xc2 | 0xc3 | 0xc5 | 0xc6 | 0xc7 | 0xc9 | 0xca | 0xcb | 0xcd | 0xce | 0xcf
    )
}

fn webp_dimensions(bytes: &[u8]) -> Option<(i32, i32)> {
    let mut index = 12;
    while index + 8 <= bytes.len() {
        let chunk_type = &bytes[index..index + 4];
        let chunk_len = u32::from_le_bytes(bytes[index + 4..index + 8].try_into().ok()?);
        let chunk_len = usize::try_from(chunk_len).ok()?;
        let data = index + 8;
        if data + chunk_len > bytes.len() {
            return None;
        }
        let chunk = &bytes[data..data + chunk_len];

        match chunk_type {
            b"VP8X" if chunk.len() >= 10 => {
                let width = 1 + read_u24_le(&chunk[4..7]);
                let height = 1 + read_u24_le(&chunk[7..10]);
                return dimensions_to_i32(width, height);
            }
            b"VP8L" if chunk.len() >= 5 && chunk[0] == 0x2f => {
                let width = 1 + (u32::from(chunk[1]) | (u32::from(chunk[2] & 0x3f) << 8));
                let height = 1
                    + ((u32::from(chunk[2] & 0xc0) >> 6)
                        | (u32::from(chunk[3]) << 2)
                        | (u32::from(chunk[4] & 0x0f) << 10));
                return dimensions_to_i32(width, height);
            }
            b"VP8 " if chunk.len() >= 10 && chunk[3..6] == [0x9d, 0x01, 0x2a] => {
                let width = u16::from_le_bytes(chunk[6..8].try_into().ok()?) & 0x3fff;
                let height = u16::from_le_bytes(chunk[8..10].try_into().ok()?) & 0x3fff;
                return dimensions_to_i32(u32::from(width), u32::from(height));
            }
            _ => {}
        }

        index = data + chunk_len + (chunk_len % 2);
    }
    None
}

fn read_u24_le(bytes: &[u8]) -> u32 {
    u32::from(bytes[0]) | (u32::from(bytes[1]) << 8) | (u32::from(bytes[2]) << 16)
}

fn dimensions_to_i32(width: u32, height: u32) -> Option<(i32, i32)> {
    if width == 0 || height == 0 {
        return None;
    }
    let width = i32::try_from(width).ok()?;
    let height = i32::try_from(height).ok()?;
    Some((width, height))
}

fn media_html(assets: &[MediaAsset], error: Option<&str>, csrf_input: &str) -> Html<String> {
    let mut body = page_start("Media Library");
    body.push_str(r#"<script src="/admin.js" defer></script>"#);
    body.push_str(
        r#"
        <section class="editor-header">
            <div>
                <p class="eyebrow">Admin</p>
                <h1>Media</h1>
            </div>
            <a href="/admin">Dashboard</a>
        </section>
        "#,
    );

    if let Some(error) = error {
        let _ = write!(body, r#"<p class="form-error">{}</p>"#, escape_html(error));
    }

    let _ = write!(
        body,
        r#"
        <form method="post" action="/admin/media" enctype="multipart/form-data" class="post-form media-upload">
            {}
            <label>
                <span>Image</span>
                <input name="file" type="file" accept="image/png,image/jpeg,image/gif,image/webp" required>
            </label>
            <label>
                <span>Alt text</span>
                <input name="alt_text" maxlength="255">
            </label>
            <div class="form-actions">
                <button type="submit">Upload image</button>
            </div>
        </form>
        "#,
        csrf_input,
    );

    if assets.is_empty() {
        body.push_str(r#"<p class="empty-state">No media assets yet.</p>"#);
    } else {
        body.push_str(r#"<section class="media-grid" aria-label="Media assets">"#);
        for asset in assets {
            let url = format!("/uploads/{}", asset.storage_path);
            let alt_text = asset.alt_text.as_deref().unwrap_or("");
            let markdown = markdown_image_snippet(&asset.storage_path, alt_text);
            let _ = write!(
                body,
                r#"
                <article class="media-card">
                    <img src="{}" alt="{}" loading="lazy">
                    <div>
                        <h2>{}</h2>
                        <p><a href="{}">{}</a></p>
                        <p>{} &middot; {} &middot; {} &middot; {}</p>
                        <div class="media-copy-tools">
                            <label>
                                <span>Public URL</span>
                                <input value="{}" readonly>
                            </label>
                            <button type="button" class="copy-value" data-copy="{}">Copy URL</button>
                            <label>
                                <span>Markdown</span>
                                <input value="{}" readonly>
                            </label>
                            <button type="button" class="copy-value" data-copy="{}">Copy Markdown</button>
                        </div>
                        <form method="post" action="/admin/media/{}" class="media-alt-form">
                            {}
                            <label>
                                <span>Alt text</span>
                                <input name="alt_text" value="{}" maxlength="255">
                            </label>
                            <button type="submit">Save alt</button>
                        </form>
                    </div>
                </article>
                "#,
                escape_html(&url),
                escape_html(alt_text),
                escape_html(&asset.original_filename),
                escape_html(&url),
                escape_html(&asset.filename),
                escape_html(&asset.mime_type),
                format_dimensions(asset.width, asset.height),
                format_size(asset.size_bytes),
                asset.created_at.format("%Y-%m-%d %H:%M UTC"),
                escape_html(&url),
                escape_html(&url),
                escape_html(&markdown),
                escape_html(&markdown),
                asset.id,
                csrf_input,
                escape_html(alt_text),
            );
        }
        body.push_str("</section>");
    }

    body.push_str(&page_end());
    Html(body)
}

fn format_size(size: i64) -> String {
    if size >= 1024 * 1024 {
        format!("{:.1} MB", size as f64 / (1024.0 * 1024.0))
    } else if size >= 1024 {
        format!("{:.1} KB", size as f64 / 1024.0)
    } else {
        format!("{size} bytes")
    }
}

fn format_dimensions(width: Option<i32>, height: Option<i32>) -> String {
    match (width, height) {
        (Some(width), Some(height)) => format!("{width} x {height}"),
        _ => "dimensions unknown".to_owned(),
    }
}

fn markdown_image_snippet(storage_path: &str, alt_text: &str) -> String {
    format!(
        "![{}](/uploads/{})",
        markdown_alt_text(alt_text),
        storage_path
    )
}

fn markdown_alt_text(value: &str) -> String {
    value.replace('\\', "\\\\").replace(']', "\\]")
}

fn server_error(error: anyhow::Error) -> Response {
    tracing::error!(?error, "media handler failed");
    server_error_page()
}

#[cfg(test)]
mod tests {
    use super::sniff_image_kind;

    #[test]
    fn sniffs_allowed_image_types() {
        let png = sniff_image_kind(
            b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00\x00\x00\x02\x00\x00\x00\x03",
        )
        .unwrap();
        assert_eq!(png.mime_type, "image/png");
        assert_eq!(png.width, Some(2));
        assert_eq!(png.height, Some(3));
        assert_eq!(
            sniff_image_kind(&[0xff, 0xd8, 0xff, 0x00])
                .unwrap()
                .mime_type,
            "image/jpeg"
        );
        assert_eq!(sniff_image_kind(b"GIF89a").unwrap().mime_type, "image/gif");
        assert_eq!(
            sniff_image_kind(b"RIFF----WEBPmore").unwrap().mime_type,
            "image/webp"
        );
        assert!(sniff_image_kind(b"not an image").is_none());
    }
}
