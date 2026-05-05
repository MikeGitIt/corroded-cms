use std::fmt::Write;

use anyhow::{Result, anyhow, bail};
use axum::{
    Form,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse, Response},
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use shared::{render_markdown, slugify, validate_slug};
use sqlx::Row;
use uuid::Uuid;

use crate::{
    AppState, auth,
    html::{escape_html, page_end, page_start, redirect},
};

const ADMIN_POST_PAGE_SIZE: i64 = 25;

#[derive(Debug)]
struct PostSummary {
    id: Uuid,
    title: String,
    slug: String,
    excerpt: String,
    status: String,
    tags: Vec<String>,
    cover_image_storage_path: Option<String>,
    cover_image_alt_text: Option<String>,
    published_at: Option<DateTime<Utc>>,
    updated_at: DateTime<Utc>,
}

#[derive(Debug)]
struct PostDetail {
    id: Uuid,
    title: String,
    slug: String,
    excerpt: String,
    body_markdown: String,
    body_html: String,
    status: String,
    cover_image_id: Option<Uuid>,
    cover_image_storage_path: Option<String>,
    cover_image_alt_text: Option<String>,
    tag_names: Vec<String>,
    tag_slugs: Vec<String>,
    published_at: Option<DateTime<Utc>>,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct PostForm {
    #[serde(default)]
    csrf_token: String,
    title: String,
    slug: String,
    excerpt: String,
    body_markdown: String,
    status: String,
    #[serde(default)]
    cover_image_id: String,
    #[serde(default)]
    tag_slugs: String,
    #[serde(default)]
    workflow_status: String,
}

#[derive(Debug, Deserialize)]
pub struct PostPreviewForm {
    #[serde(default)]
    csrf_token: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    excerpt: String,
    #[serde(default)]
    body_markdown: String,
    #[serde(default)]
    cover_image_id: String,
}

#[derive(Debug)]
struct MediaOption {
    id: Uuid,
    original_filename: String,
    storage_path: String,
    alt_text: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PostListParams {
    status: Option<String>,
    q: Option<String>,
    page: Option<u32>,
}

#[derive(Debug)]
struct PostListFilters {
    status: Option<String>,
    q: Option<String>,
    page: u32,
}

pub async fn admin_list(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<PostListParams>,
) -> Response {
    if auth::current_admin(&state, &headers).await.is_none() {
        return redirect("/admin/login");
    }

    let filters = match normalize_post_list_params(params) {
        Ok(filters) => filters,
        Err(error) => return (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    };

    match list_admin_posts(&state, &filters).await {
        Ok(posts) => admin_list_html(&posts, &filters).into_response(),
        Err(error) => server_error(error),
    }
}

pub async fn admin_new(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if auth::current_admin(&state, &headers).await.is_none() {
        return redirect("/admin/login");
    }

    match list_media_options(&state).await {
        Ok(media) => post_form_html(
            "New Post",
            "/admin/posts",
            None,
            &media,
            None,
            &auth::csrf_input(&state, &headers),
        )
        .into_response(),
        Err(error) => server_error(error),
    }
}

pub async fn admin_create(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<PostForm>,
) -> Response {
    let Some(user) = auth::current_admin(&state, &headers).await else {
        return redirect("/admin/login");
    };
    let csrf_input = auth::csrf_input(&state, &headers);
    if !auth::verify_csrf(&state, &headers, &form.csrf_token) {
        return auth::csrf_rejection();
    }

    match create_post(&state, &user, form).await {
        Ok(id) => redirect(format!("/admin/posts/{id}/edit")),
        Err(error) => match list_media_options(&state).await {
            Ok(media) => post_form_html(
                "New Post",
                "/admin/posts",
                None,
                &media,
                Some(&error.to_string()),
                &csrf_input,
            )
            .into_response(),
            Err(_) => server_error(error),
        },
    }
}

pub async fn admin_edit(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    if auth::current_admin(&state, &headers).await.is_none() {
        return redirect("/admin/login");
    }

    match get_post_for_admin(&state, id).await {
        Ok(Some(post)) => match list_media_options(&state).await {
            Ok(media) => {
                let action = format!("/admin/posts/{id}");
                post_form_html(
                    "Edit Post",
                    &action,
                    Some(&post),
                    &media,
                    None,
                    &auth::csrf_input(&state, &headers),
                )
                .into_response()
            }
            Err(error) => server_error(error),
        },
        Ok(None) => not_found(),
        Err(error) => server_error(error),
    }
}

pub async fn admin_update(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Form(form): Form<PostForm>,
) -> Response {
    if auth::current_admin(&state, &headers).await.is_none() {
        return redirect("/admin/login");
    }
    let csrf_input = auth::csrf_input(&state, &headers);
    if !auth::verify_csrf(&state, &headers, &form.csrf_token) {
        return auth::csrf_rejection();
    }

    match update_post(&state, id, form).await {
        Ok(()) => redirect(format!("/admin/posts/{id}/edit")),
        Err(error) => match get_post_for_admin(&state, id).await {
            Ok(post) => match list_media_options(&state).await {
                Ok(media) => {
                    let action = format!("/admin/posts/{id}");
                    post_form_html(
                        "Edit Post",
                        &action,
                        post.as_ref(),
                        &media,
                        Some(&error.to_string()),
                        &csrf_input,
                    )
                    .into_response()
                }
                Err(_) => server_error(error),
            },
            Err(_) => server_error(error),
        },
    }
}

pub async fn admin_preview(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<PostPreviewForm>,
) -> Response {
    if auth::current_admin(&state, &headers).await.is_none() {
        return redirect("/admin/login");
    }
    if !auth::verify_csrf(&state, &headers, &form.csrf_token) {
        return auth::csrf_rejection();
    }
    if form.body_markdown.len() > 1024 * 1024 {
        return (StatusCode::BAD_REQUEST, "body must be 1 MiB or smaller").into_response();
    }

    let cover_image = match preview_cover_image(&state, &form.cover_image_id).await {
        Ok(cover_image) => cover_image,
        Err(error) => return (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    };

    markdown_preview_html(
        &form.title,
        &form.excerpt,
        &render_markdown(&form.body_markdown),
        cover_image.as_ref(),
    )
    .into_response()
}

pub async fn admin_archive(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Form(form): Form<auth::CsrfForm>,
) -> Response {
    if auth::current_admin(&state, &headers).await.is_none() {
        return redirect("/admin/login");
    }
    if !auth::verify_csrf(&state, &headers, &form.csrf_token) {
        return auth::csrf_rejection();
    }

    match set_status(&state, id, "archived").await {
        Ok(()) => redirect("/admin/posts"),
        Err(error) => server_error(error),
    }
}

pub async fn admin_publish(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Form(form): Form<auth::CsrfForm>,
) -> Response {
    if auth::current_admin(&state, &headers).await.is_none() {
        return redirect("/admin/login");
    }
    if !auth::verify_csrf(&state, &headers, &form.csrf_token) {
        return auth::csrf_rejection();
    }

    match set_status(&state, id, "published").await {
        Ok(()) => redirect(format!("/admin/posts/{id}/edit")),
        Err(error) => server_error(error),
    }
}

pub async fn admin_unpublish(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Form(form): Form<auth::CsrfForm>,
) -> Response {
    if auth::current_admin(&state, &headers).await.is_none() {
        return redirect("/admin/login");
    }
    if !auth::verify_csrf(&state, &headers, &form.csrf_token) {
        return auth::csrf_rejection();
    }

    match set_status(&state, id, "draft").await {
        Ok(()) => redirect(format!("/admin/posts/{id}/edit")),
        Err(error) => server_error(error),
    }
}

pub async fn home(State(state): State<AppState>) -> Response {
    match list_public_posts_with_limit(&state, 5).await {
        Ok(posts) => public_home_html(&state, &posts).into_response(),
        Err(error) => server_error(error),
    }
}

pub async fn blog_index(State(state): State<AppState>) -> Response {
    match list_public_posts(&state).await {
        Ok(posts) => public_index_html(&posts).into_response(),
        Err(error) => server_error(error),
    }
}

pub async fn blog_detail(State(state): State<AppState>, Path(slug): Path<String>) -> Response {
    match get_public_post(&state, &slug).await {
        Ok(Some(post)) => public_detail_html(&state, &post).into_response(),
        Ok(None) => not_found(),
        Err(error) => server_error(error),
    }
}

pub async fn tag_detail(State(state): State<AppState>, Path(slug): Path<String>) -> Response {
    match list_public_posts_for_tag(&state, &slug).await {
        Ok(Some((tag_name, posts))) if !posts.is_empty() => {
            public_tag_html(&tag_name, &slug, &posts).into_response()
        }
        Ok(_) => not_found(),
        Err(error) => server_error(error),
    }
}

async fn list_admin_posts(state: &AppState, filters: &PostListFilters) -> Result<Vec<PostSummary>> {
    let offset = (filters.page.saturating_sub(1) as i64) * ADMIN_POST_PAGE_SIZE;
    let rows = sqlx::query(
        r#"
        SELECT posts.id,
               posts.title,
               posts.slug,
               posts.excerpt,
               posts.status,
               posts.published_at,
               posts.updated_at,
               cover_image.storage_path AS cover_image_storage_path,
               cover_image.alt_text AS cover_image_alt_text,
               COALESCE(array_agg(tags.name ORDER BY tags.name)
                   FILTER (WHERE tags.id IS NOT NULL), ARRAY[]::text[]) AS tags
        FROM posts
        LEFT JOIN media_assets cover_image ON cover_image.id = posts.cover_image_id
        LEFT JOIN post_tags ON post_tags.post_id = posts.id
        LEFT JOIN tags ON tags.id = post_tags.tag_id
        WHERE ($1::text IS NULL OR posts.status = $1)
          AND (
              $2::text IS NULL
              OR posts.title ILIKE '%' || $2 || '%'
              OR posts.slug ILIKE '%' || $2 || '%'
              OR posts.excerpt ILIKE '%' || $2 || '%'
          )
        GROUP BY posts.id, cover_image.id
        ORDER BY posts.updated_at DESC
        LIMIT $3 OFFSET $4
        "#,
    )
    .bind(filters.status.as_deref())
    .bind(filters.q.as_deref())
    .bind(ADMIN_POST_PAGE_SIZE)
    .bind(offset)
    .fetch_all(&state.pool)
    .await?;

    rows.into_iter().map(post_summary_from_row).collect()
}

fn normalize_post_list_params(params: PostListParams) -> Result<PostListFilters> {
    let status = match params.status.as_deref().map(str::trim) {
        Some("") | None | Some("all") => None,
        Some(status @ ("draft" | "published" | "archived")) => Some(status.to_owned()),
        Some(_) => bail!("post status filter is invalid"),
    };

    let q = params
        .q
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    if q.as_deref().map(str::len).unwrap_or(0) > 100 {
        bail!("search query must be 100 characters or fewer");
    }

    let page = params.page.unwrap_or(1).clamp(1, 1000);
    Ok(PostListFilters { status, q, page })
}

async fn list_public_posts(state: &AppState) -> Result<Vec<PostSummary>> {
    list_public_posts_with_limit(state, 10).await
}

async fn list_public_posts_with_limit(state: &AppState, limit: i64) -> Result<Vec<PostSummary>> {
    let rows = sqlx::query(
        r#"
        SELECT posts.id,
               posts.title,
               posts.slug,
               posts.excerpt,
               posts.status,
               posts.published_at,
               posts.updated_at,
               cover_image.storage_path AS cover_image_storage_path,
               cover_image.alt_text AS cover_image_alt_text,
               COALESCE(array_agg(tags.name ORDER BY tags.name)
                   FILTER (WHERE tags.id IS NOT NULL AND tags.archived_at IS NULL), ARRAY[]::text[]) AS tags
        FROM posts
        LEFT JOIN media_assets cover_image ON cover_image.id = posts.cover_image_id
        LEFT JOIN post_tags ON post_tags.post_id = posts.id
        LEFT JOIN tags ON tags.id = post_tags.tag_id
        WHERE posts.status = 'published'
          AND posts.published_at IS NOT NULL
          AND posts.published_at <= now()
        GROUP BY posts.id, cover_image.id
        ORDER BY posts.published_at DESC, posts.created_at DESC
        LIMIT $1
        "#,
    )
    .bind(limit)
    .fetch_all(&state.pool)
    .await?;

    rows.into_iter().map(post_summary_from_row).collect()
}

async fn list_public_posts_for_tag(
    state: &AppState,
    tag_slug: &str,
) -> Result<Option<(String, Vec<PostSummary>)>> {
    let tag = sqlx::query(
        r#"
        SELECT id, name
        FROM tags
        WHERE slug = $1 AND archived_at IS NULL
        "#,
    )
    .bind(tag_slug)
    .fetch_optional(&state.pool)
    .await?;

    let Some(tag) = tag else {
        return Ok(None);
    };
    let tag_id: Uuid = tag.try_get("id")?;
    let tag_name: String = tag.try_get("name")?;

    let rows = sqlx::query(
        r#"
        SELECT posts.id,
               posts.title,
               posts.slug,
               posts.excerpt,
               posts.status,
               posts.published_at,
               posts.updated_at,
               cover_image.storage_path AS cover_image_storage_path,
               cover_image.alt_text AS cover_image_alt_text,
               COALESCE(array_agg(all_tags.name ORDER BY all_tags.name)
                   FILTER (WHERE all_tags.id IS NOT NULL AND all_tags.archived_at IS NULL), ARRAY[]::text[]) AS tags
        FROM posts
        LEFT JOIN media_assets cover_image ON cover_image.id = posts.cover_image_id
        JOIN post_tags selected_tag ON selected_tag.post_id = posts.id
        LEFT JOIN post_tags all_post_tags ON all_post_tags.post_id = posts.id
        LEFT JOIN tags all_tags ON all_tags.id = all_post_tags.tag_id
        WHERE selected_tag.tag_id = $1
          AND posts.status = 'published'
          AND posts.published_at IS NOT NULL
          AND posts.published_at <= now()
        GROUP BY posts.id, cover_image.id
        ORDER BY posts.published_at DESC, posts.created_at DESC
        "#,
    )
    .bind(tag_id)
    .fetch_all(&state.pool)
    .await?;

    Ok(Some((
        tag_name,
        rows.into_iter()
            .map(post_summary_from_row)
            .collect::<Result<Vec<_>>>()?,
    )))
}

async fn get_post_for_admin(state: &AppState, id: Uuid) -> Result<Option<PostDetail>> {
    let row = sqlx::query(
        r#"
        SELECT posts.id,
               posts.title,
               posts.slug,
               posts.excerpt,
               posts.body_markdown,
               posts.body_html,
               posts.status,
               posts.cover_image_id,
               cover_image.storage_path AS cover_image_storage_path,
               cover_image.alt_text AS cover_image_alt_text,
               posts.published_at,
               posts.updated_at,
               COALESCE(array_agg(tags.name ORDER BY tags.name)
                   FILTER (WHERE tags.id IS NOT NULL), ARRAY[]::text[]) AS tag_names,
               COALESCE(array_agg(tags.slug ORDER BY tags.name)
                   FILTER (WHERE tags.id IS NOT NULL), ARRAY[]::text[]) AS tag_slugs
        FROM posts
        LEFT JOIN media_assets cover_image ON cover_image.id = posts.cover_image_id
        LEFT JOIN post_tags ON post_tags.post_id = posts.id
        LEFT JOIN tags ON tags.id = post_tags.tag_id
        WHERE posts.id = $1
        GROUP BY posts.id, cover_image.id
        "#,
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await?;

    row.map(post_detail_from_row).transpose()
}

async fn get_public_post(state: &AppState, slug: &str) -> Result<Option<PostDetail>> {
    let row = sqlx::query(
        r#"
        SELECT posts.id,
               posts.title,
               posts.slug,
               posts.excerpt,
               posts.body_markdown,
               posts.body_html,
               posts.status,
               posts.cover_image_id,
               cover_image.storage_path AS cover_image_storage_path,
               cover_image.alt_text AS cover_image_alt_text,
               posts.published_at,
               posts.updated_at,
               COALESCE(array_agg(tags.name ORDER BY tags.name)
                   FILTER (WHERE tags.id IS NOT NULL AND tags.archived_at IS NULL), ARRAY[]::text[]) AS tag_names,
               COALESCE(array_agg(tags.slug ORDER BY tags.name)
                   FILTER (WHERE tags.id IS NOT NULL AND tags.archived_at IS NULL), ARRAY[]::text[]) AS tag_slugs
        FROM posts
        LEFT JOIN media_assets cover_image ON cover_image.id = posts.cover_image_id
        LEFT JOIN post_tags ON post_tags.post_id = posts.id
        LEFT JOIN tags ON tags.id = post_tags.tag_id
        WHERE posts.slug = $1
          AND posts.status = 'published'
          AND posts.published_at IS NOT NULL
          AND posts.published_at <= now()
        GROUP BY posts.id, cover_image.id
        "#,
    )
    .bind(slug)
    .fetch_optional(&state.pool)
    .await?;

    row.map(post_detail_from_row).transpose()
}

async fn list_media_options(state: &AppState) -> Result<Vec<MediaOption>> {
    let rows = sqlx::query(
        r#"
        SELECT id, original_filename, storage_path, alt_text
        FROM media_assets
        ORDER BY created_at DESC
        LIMIT 100
        "#,
    )
    .fetch_all(&state.pool)
    .await?;

    rows.into_iter()
        .map(|row| {
            Ok(MediaOption {
                id: row.try_get("id")?,
                original_filename: row.try_get("original_filename")?,
                storage_path: row.try_get("storage_path")?,
                alt_text: row.try_get("alt_text")?,
            })
        })
        .collect()
}

async fn preview_cover_image(state: &AppState, cover_image_id: &str) -> Result<Option<MediaOption>> {
    let cover_image_id = cover_image_id.trim();
    if cover_image_id.is_empty() {
        return Ok(None);
    }

    let id = Uuid::parse_str(cover_image_id).map_err(|_| anyhow!("cover image is invalid"))?;
    let row = sqlx::query(
        r#"
        SELECT id, original_filename, storage_path, alt_text
        FROM media_assets
        WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await?;

    let Some(row) = row else {
        bail!("cover image was not found");
    };

    Ok(Some(MediaOption {
        id: row.try_get("id")?,
        original_filename: row.try_get("original_filename")?,
        storage_path: row.try_get("storage_path")?,
        alt_text: row.try_get("alt_text")?,
    }))
}

async fn create_post(state: &AppState, user: &auth::AdminUser, form: PostForm) -> Result<Uuid> {
    let input = validate_post_form(form)?;
    let body_html = render_markdown(&input.body_markdown);
    let published_at: Option<DateTime<Utc>> = (input.status == "published").then(Utc::now);
    let tags = input.tags.clone();

    let id = sqlx::query_scalar::<_, Uuid>(
        r#"
        INSERT INTO posts (title, slug, excerpt, body_markdown, body_html, status, author_id, published_at, cover_image_id)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        RETURNING id
        "#,
    )
    .bind(input.title)
    .bind(input.slug)
    .bind(input.excerpt)
    .bind(input.body_markdown)
    .bind(body_html)
    .bind(input.status)
    .bind(user.id)
    .bind(published_at)
    .bind(input.cover_image_id)
    .fetch_one(&state.pool)
    .await?;

    sync_post_tags(state, id, &tags).await?;
    Ok(id)
}

async fn update_post(state: &AppState, id: Uuid, form: PostForm) -> Result<()> {
    let input = validate_post_form(form)?;
    let body_html = render_markdown(&input.body_markdown);
    let tags = input.tags.clone();

    sqlx::query(
        r#"
        UPDATE posts
        SET title = $1,
            slug = $2,
            excerpt = $3,
            body_markdown = $4,
            body_html = $5,
            status = $6,
            cover_image_id = $7,
            published_at = CASE
                WHEN $6 = 'published' AND published_at IS NULL THEN now()
                ELSE published_at
            END,
            updated_at = now()
        WHERE id = $8
        "#,
    )
    .bind(input.title)
    .bind(input.slug)
    .bind(input.excerpt)
    .bind(input.body_markdown)
    .bind(body_html)
    .bind(input.status)
    .bind(input.cover_image_id)
    .bind(id)
    .execute(&state.pool)
    .await?;

    sync_post_tags(state, id, &tags).await?;
    Ok(())
}

async fn set_status(state: &AppState, id: Uuid, status: &str) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE posts
        SET status = $1,
            published_at = CASE
                WHEN $1 = 'published' AND published_at IS NULL THEN now()
                ELSE published_at
            END,
            updated_at = now()
        WHERE id = $2
        "#,
    )
    .bind(status)
    .bind(id)
    .execute(&state.pool)
    .await?;
    Ok(())
}

struct ValidPostForm {
    title: String,
    slug: String,
    excerpt: String,
    body_markdown: String,
    status: String,
    cover_image_id: Option<Uuid>,
    tags: Vec<(String, String)>,
}

fn validate_post_form(form: PostForm) -> Result<ValidPostForm> {
    let title = form.title.trim().to_owned();
    if title.is_empty() || title.chars().count() > 200 {
        bail!("title must be 1 to 200 characters");
    }

    let slug = if form.slug.trim().is_empty() {
        slugify(&title)
    } else {
        slugify(&form.slug)
    };
    validate_slug(&slug, 200).map_err(|error| anyhow!("{error}"))?;

    let excerpt = form.excerpt.trim().to_owned();
    if excerpt.chars().count() > 500 {
        bail!("excerpt must be 500 characters or fewer");
    }

    if form.body_markdown.len() > 1024 * 1024 {
        bail!("body must be 1 MiB or smaller");
    }

    let workflow_status = form.workflow_status.trim();
    let status = if workflow_status.is_empty() {
        form.status.trim().to_owned()
    } else {
        workflow_status.to_owned()
    };
    if !matches!(status.as_str(), "draft" | "published" | "archived") {
        bail!("status is invalid");
    }

    let cover_image_id = match form.cover_image_id.trim() {
        "" => None,
        value => Some(Uuid::parse_str(value).map_err(|_| anyhow!("cover image is invalid"))?),
    };

    let tags = parse_tags(&form.tag_slugs)?;

    Ok(ValidPostForm {
        title,
        slug,
        excerpt,
        body_markdown: form.body_markdown,
        status,
        cover_image_id,
        tags,
    })
}

fn parse_tags(input: &str) -> Result<Vec<(String, String)>> {
    let mut tags = Vec::new();
    for raw in input.split(',') {
        let name = raw.trim();
        if name.is_empty() {
            continue;
        }
        if name.chars().count() > 80 {
            bail!("tag names must be 80 characters or fewer");
        }
        let slug = slugify(name);
        validate_slug(&slug, 100).map_err(|error| anyhow!("{error}"))?;
        if !tags.iter().any(|(_, existing_slug)| existing_slug == &slug) {
            tags.push((name.to_owned(), slug));
        }
    }
    Ok(tags)
}

async fn sync_post_tags(state: &AppState, post_id: Uuid, tags: &[(String, String)]) -> Result<()> {
    sqlx::query("DELETE FROM post_tags WHERE post_id = $1")
        .bind(post_id)
        .execute(&state.pool)
        .await?;

    for (name, slug) in tags {
        let tag_id = sqlx::query_scalar::<_, Uuid>(
            r#"
            INSERT INTO tags (name, slug)
            VALUES ($1, $2)
            ON CONFLICT (slug) DO UPDATE
            SET updated_at = tags.updated_at
            RETURNING id
            "#,
        )
        .bind(name)
        .bind(slug)
        .fetch_one(&state.pool)
        .await?;

        sqlx::query(
            r#"
            INSERT INTO post_tags (post_id, tag_id)
            VALUES ($1, $2)
            ON CONFLICT DO NOTHING
            "#,
        )
        .bind(post_id)
        .bind(tag_id)
        .execute(&state.pool)
        .await?;
    }

    Ok(())
}

fn post_summary_from_row(row: sqlx::postgres::PgRow) -> Result<PostSummary> {
    Ok(PostSummary {
        id: row.try_get("id")?,
        title: row.try_get("title")?,
        slug: row.try_get("slug")?,
        excerpt: row.try_get("excerpt")?,
        status: row.try_get("status")?,
        tags: row.try_get("tags")?,
        cover_image_storage_path: row.try_get("cover_image_storage_path")?,
        cover_image_alt_text: row.try_get("cover_image_alt_text")?,
        published_at: row.try_get("published_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn post_detail_from_row(row: sqlx::postgres::PgRow) -> Result<PostDetail> {
    Ok(PostDetail {
        id: row.try_get("id")?,
        title: row.try_get("title")?,
        slug: row.try_get("slug")?,
        excerpt: row.try_get("excerpt")?,
        body_markdown: row.try_get("body_markdown")?,
        body_html: row.try_get("body_html")?,
        status: row.try_get("status")?,
        cover_image_id: row.try_get("cover_image_id")?,
        cover_image_storage_path: row.try_get("cover_image_storage_path")?,
        cover_image_alt_text: row.try_get("cover_image_alt_text")?,
        tag_names: row.try_get("tag_names")?,
        tag_slugs: row.try_get("tag_slugs")?,
        published_at: row.try_get("published_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn admin_list_html(posts: &[PostSummary], filters: &PostListFilters) -> Html<String> {
    let mut body = page_start("Posts");
    body.push_str(
        r#"
        <section class="admin-dashboard">
            <div>
                <p class="eyebrow">Admin</p>
                <h1>Posts</h1>
            </div>
            <a class="button-link" href="/admin/posts/new">New post</a>
        </section>
        "#,
    );

    let selected_status = filters.status.as_deref().unwrap_or("all");
    let _ = write!(
        body,
        r#"
        <form method="get" action="/admin/posts" class="admin-filters">
            <label>
                <span>Status</span>
                <select name="status">
                    {}
                    {}
                    {}
                    {}
                </select>
            </label>
            <label>
                <span>Search</span>
                <input name="q" value="{}" maxlength="100" placeholder="Title, slug, or excerpt">
            </label>
            <button type="submit">Filter</button>
            <a href="/admin/posts">Clear</a>
        </form>
        "#,
        selected_option("all", "All", selected_status),
        selected_option("draft", "Draft", selected_status),
        selected_option("published", "Published", selected_status),
        selected_option("archived", "Archived", selected_status),
        escape_html(filters.q.as_deref().unwrap_or("")),
    );

    if posts.is_empty() {
        body.push_str(r#"<p class="empty-state">No posts yet.</p>"#);
    } else {
        body.push_str(r#"<div class="table-wrap"><table class="admin-table"><thead><tr><th>Title</th><th>Status</th><th>Published</th><th>Updated</th><th></th></tr></thead><tbody>"#);
        for post in posts {
            let published = post
                .published_at
                .map(format_date)
                .unwrap_or_else(|| "-".to_owned());
            let _ = write!(
                body,
                r#"<tr><td><a href="/admin/posts/{}/edit">{}</a><span>{}</span></td><td>{}</td><td>{}</td><td>{}</td><td><a href="/blog/{}">View</a></td></tr>"#,
                post.id,
                escape_html(&post.title),
                escape_html(&post.slug),
                escape_html(&post.status),
                published,
                format_date(post.updated_at),
                escape_html(&post.slug),
            );
        }
        body.push_str("</tbody></table></div>");
    }

    body.push_str(&page_end());
    Html(body)
}

fn post_form_html(
    title: &str,
    action: &str,
    post: Option<&PostDetail>,
    media_options: &[MediaOption],
    error: Option<&str>,
    csrf_input: &str,
) -> Html<String> {
    let mut body = page_start(title);
    let default_title = post.map(|p| p.title.as_str()).unwrap_or("");
    let default_slug = post.map(|p| p.slug.as_str()).unwrap_or("");
    let default_excerpt = post.map(|p| p.excerpt.as_str()).unwrap_or("");
    let default_body = post.map(|p| p.body_markdown.as_str()).unwrap_or("");
    let default_status = post.map(|p| p.status.as_str()).unwrap_or("draft");
    let default_cover_image_id = post.and_then(|p| p.cover_image_id);
    let default_tags = post.map(|p| p.tag_slugs.join(", ")).unwrap_or_default();
    let media_insert_tools = media_insert_tools_html(media_options);
    let media_options = media_options_html(media_options, default_cover_image_id);
    let workflow_button = workflow_submit_button(post);

    let _ = write!(
        body,
        r#"
        <section class="editor-header">
            <div>
                <p class="eyebrow">Admin</p>
                <h1>{}</h1>
            </div>
            <a href="/admin/posts">Back to posts</a>
        </section>
        "#,
        escape_html(title)
    );

    if let Some(error) = error {
        let _ = write!(body, r#"<p class="form-error">{}</p>"#, escape_html(error));
    }

    let _ = write!(
        body,
        r#"
        <form method="post" action="{}" class="post-form">
            {}
            <label>
                <span>Title</span>
                <input name="title" value="{}" maxlength="200" required>
            </label>
            <label>
                <span>Slug</span>
                <input name="slug" value="{}" maxlength="200" placeholder="generated from title">
            </label>
            <label>
                <span>Excerpt</span>
                <textarea name="excerpt" maxlength="500" rows="3">{}</textarea>
            </label>
            <label>
                <span>Status</span>
                <select name="status">
                    {}
                    {}
                    {}
                </select>
            </label>
            <label>
                <span>Cover image</span>
                <select name="cover_image_id">
                    <option value="">No cover image</option>
                    {}
                </select>
            </label>
            <label>
                <span>Tags</span>
                <input name="tag_slugs" value="{}" placeholder="rust, cms, release">
            </label>
            <label>
                <span>Markdown</span>
                <textarea name="body_markdown" rows="18">{}</textarea>
            </label>
            {}
            <div class="form-actions">
                <button type="submit">Save</button>
                <button type="submit" formaction="/admin/posts/preview" formtarget="_blank" formnovalidate>Preview</button>
                {}
            </div>
        </form>
        "#,
        escape_html(action),
        csrf_input,
        escape_html(default_title),
        escape_html(default_slug),
        escape_html(default_excerpt),
        selected_option("draft", "Draft", default_status),
        selected_option("published", "Published", default_status),
        selected_option("archived", "Archived", default_status),
        media_options,
        escape_html(&default_tags),
        escape_html(default_body),
        media_insert_tools,
        workflow_button,
    );

    if let Some(post) = post {
        let _ = write!(
            body,
            r#"<p class="editor-meta">Updated {}</p>"#,
            format_date(post.updated_at)
        );
        let _ = write!(
            body,
            r#"
            <form method="post" action="/admin/posts/{}/archive" class="danger-form">
                {}
                <button type="submit">Archive</button>
            </form>
            "#,
            post.id, csrf_input,
        );
    }

    body.push_str(&page_end());
    Html(body)
}

fn public_home_html(state: &AppState, posts: &[PostSummary]) -> Html<String> {
    let mut body = page_start(&state.config.site_name);
    let _ = write!(
        body,
        r#"
        <section class="page-header">
            <p class="eyebrow">{}</p>
            <h1>{}</h1>
            <p>{}</p>
            <p><a class="button-link" href="/blog">View all posts</a></p>
        </section>
        <section class="post-list">
        "#,
        escape_html(&state.config.site_name),
        escape_html(&state.config.site_name),
        escape_html(&state.config.site_description),
    );

    if posts.is_empty() {
        body.push_str(r#"<p class="empty-state">No published posts yet.</p>"#);
    } else {
        for post in posts {
            let _ = write!(
                body,
                r#"
                <article class="post-card">
                    {}
                    <p>{}</p>
                    <h2><a href="/blog/{}">{}</a></h2>
                    <p>{}</p>
                    {}
                </article>
                "#,
                cover_image_html(
                    post.cover_image_storage_path.as_deref(),
                    post.cover_image_alt_text.as_deref(),
                    "post-card-cover",
                ),
                post.published_at
                    .map(format_date)
                    .unwrap_or_else(|| "Draft".to_owned()),
                escape_html(&post.slug),
                escape_html(&post.title),
                escape_html(&post.excerpt),
                tag_links(&post.tags),
            );
        }
    }

    body.push_str("</section>");
    body.push_str(&page_end());
    Html(body)
}

fn markdown_preview_html(
    title: &str,
    excerpt: &str,
    body_html: &str,
    cover_image: Option<&MediaOption>,
) -> Html<String> {
    let title = title.trim();
    let title = if title.is_empty() {
        "Markdown Preview"
    } else {
        title
    };
    let excerpt = excerpt.trim();
    let cover_image = cover_image
        .map(|media| {
            cover_image_html_with_loading(
                Some(media.storage_path.as_str()),
                media.alt_text.as_deref(),
                "post-cover",
                "eager",
                Some("high"),
            )
        })
        .unwrap_or_default();
    let mut body = page_start(title);
    let _ = write!(
        body,
        r#"
        <article class="post-detail">
            <p class="eyebrow">Preview</p>
            <h1>{}</h1>
            {}
            {}
            <div class="post-body">{}</div>
        </article>
        "#,
        escape_html(title),
        preview_excerpt_html(excerpt),
        cover_image,
        body_html,
    );
    body.push_str(&page_end());
    Html(body)
}

fn preview_excerpt_html(excerpt: &str) -> String {
    if excerpt.is_empty() {
        return String::new();
    }
    format!(r#"<p>{}</p>"#, escape_html(excerpt))
}

fn public_index_html(posts: &[PostSummary]) -> Html<String> {
    let mut body = page_start("Blog");
    body.push_str(
        r#"
        <section class="page-header">
            <p class="eyebrow">Blog</p>
            <h1>Latest posts</h1>
        </section>
        <section class="post-list">
        "#,
    );

    if posts.is_empty() {
        body.push_str(r#"<p class="empty-state">No published posts yet.</p>"#);
    } else {
        for post in posts {
            let _ = write!(
                body,
                r#"
                <article class="post-card">
                    {}
                    <p>{}</p>
                    <h2><a href="/blog/{}">{}</a></h2>
                    <p>{}</p>
                    {}
                </article>
                "#,
                cover_image_html(
                    post.cover_image_storage_path.as_deref(),
                    post.cover_image_alt_text.as_deref(),
                    "post-card-cover",
                ),
                post.published_at
                    .map(format_date)
                    .unwrap_or_else(|| "Draft".to_owned()),
                escape_html(&post.slug),
                escape_html(&post.title),
                escape_html(&post.excerpt),
                tag_links(&post.tags),
            );
        }
    }

    body.push_str("</section>");
    body.push_str(&page_end());
    Html(body)
}

fn public_detail_html(state: &AppState, post: &PostDetail) -> Html<String> {
    let base_url = state.config.base_url.trim_end_matches('/');
    let canonical = format!("{base_url}/blog/{}", post.slug);
    let page_title = format!("{} | {}", post.title, state.config.site_name);
    let image_url = post
        .cover_image_storage_path
        .as_ref()
        .map(|path| format!("{base_url}/uploads/{path}"));
    let image_meta = social_image_meta(image_url.as_deref(), post.cover_image_alt_text.as_deref());
    let twitter_card = if image_url.is_some() {
        "summary_large_image"
    } else {
        "summary"
    };
    let mut body = format!(
        r#"<!DOCTYPE html>
        <html lang="en">
        <head>
            <meta charset="utf-8">
            <meta name="viewport" content="width=device-width, initial-scale=1">
            <title>{}</title>
            <meta name="description" content="{}">
            <link rel="canonical" href="{}">
            <meta property="og:type" content="article">
            <meta property="og:site_name" content="{}">
            <meta property="og:title" content="{}">
            <meta property="og:description" content="{}">
            <meta property="og:url" content="{}">
            <meta name="twitter:card" content="{}">
            <meta name="twitter:title" content="{}">
            <meta name="twitter:description" content="{}">
            {}
            <link rel="alternate" type="application/rss+xml" href="/feed.xml">
            <link rel="stylesheet" href="/pkg/corroded-cms.css">
        </head>
        <body>
            <div class="app-shell">
                <header class="site-header">
                    <a class="brand" href="/">Corroded CMS</a>
                    <nav class="site-nav" aria-label="Primary">
                        <a href="/">Home</a>
                        <a href="/blog">Blog</a>
                        <a href="/admin">Admin</a>
                    </nav>
                </header>
                <main class="site-main">
        "#,
        escape_html(&page_title),
        escape_html(&post.excerpt),
        escape_html(&canonical),
        escape_html(&state.config.site_name),
        escape_html(&post.title),
        escape_html(&post.excerpt),
        escape_html(&canonical),
        twitter_card,
        escape_html(&post.title),
        escape_html(&post.excerpt),
        image_meta,
    );

    let _ = write!(
        body,
        r#"
        <article class="post-detail">
            <p class="eyebrow">{}</p>
            <h1>{}</h1>
            {}
            {}
            <div class="post-body">{}</div>
        </article>
        "#,
        post.published_at
            .map(format_date)
            .unwrap_or_else(|| "Unpublished".to_owned()),
        escape_html(&post.title),
        tag_links_with_slugs(&post.tag_names, &post.tag_slugs),
        cover_image_html_with_loading(
            post.cover_image_storage_path.as_deref(),
            post.cover_image_alt_text.as_deref(),
            "post-cover",
            "eager",
            Some("high"),
        ),
        post.body_html,
    );

    body.push_str(&page_end());
    Html(body)
}

fn public_tag_html(tag_name: &str, slug: &str, posts: &[PostSummary]) -> Html<String> {
    let mut body = page_start(tag_name);
    let _ = write!(
        body,
        r#"
        <section class="page-header">
            <p class="eyebrow">Tag</p>
            <h1>{}</h1>
        </section>
        <section class="post-list" data-tag="{}">
        "#,
        escape_html(tag_name),
        escape_html(slug)
    );

    for post in posts {
        let _ = write!(
            body,
            r#"
            <article class="post-card">
                {}
                <p>{}</p>
                <h2><a href="/blog/{}">{}</a></h2>
                <p>{}</p>
                {}
            </article>
            "#,
            cover_image_html(
                post.cover_image_storage_path.as_deref(),
                post.cover_image_alt_text.as_deref(),
                "post-card-cover",
            ),
            post.published_at
                .map(format_date)
                .unwrap_or_else(|| "Draft".to_owned()),
            escape_html(&post.slug),
            escape_html(&post.title),
            escape_html(&post.excerpt),
            tag_links(&post.tags),
        );
    }

    body.push_str("</section>");
    body.push_str(&page_end());
    Html(body)
}

fn tag_links(tags: &[String]) -> String {
    if tags.is_empty() {
        return String::new();
    }
    let mut body = String::from(r#"<div class="tag-list">"#);
    for tag in tags {
        let _ = write!(body, r#"<span>{}</span>"#, escape_html(tag));
    }
    body.push_str("</div>");
    body
}

fn tag_links_with_slugs(names: &[String], slugs: &[String]) -> String {
    if names.is_empty() {
        return String::new();
    }
    let mut body = String::from(r#"<div class="tag-list">"#);
    for (name, slug) in names.iter().zip(slugs) {
        let _ = write!(
            body,
            r#"<a href="/tags/{}">{}</a>"#,
            escape_html(slug),
            escape_html(name)
        );
    }
    body.push_str("</div>");
    body
}

fn media_insert_tools_html(media_options: &[MediaOption]) -> String {
    if media_options.is_empty() {
        return String::new();
    }

    let mut body = String::from(
        r#"<section class="media-insert-tools" aria-label="Image Markdown snippets"><h2>Image snippets</h2><div class="media-insert-list">"#,
    );
    for media in media_options {
        let alt_text = media.alt_text.as_deref().unwrap_or("");
        let markdown = format!(
            "![{}](/uploads/{})",
            markdown_alt_text(alt_text),
            media.storage_path
        );
        let _ = write!(
            body,
            r#"<label><span>{}</span><input value="{}" readonly></label>"#,
            escape_html(&media.original_filename),
            escape_html(&markdown),
        );
    }
    body.push_str("</div></section>");
    body
}

fn media_options_html(media_options: &[MediaOption], current: Option<Uuid>) -> String {
    let mut body = String::new();
    for media in media_options {
        let selected = (Some(media.id) == current)
            .then_some(" selected")
            .unwrap_or("");
        let label = match media.alt_text.as_deref().filter(|value| !value.is_empty()) {
            Some(alt_text) => format!(
                "{} - {} (/uploads/{})",
                media.original_filename, alt_text, media.storage_path
            ),
            None => format!(
                "{} (/uploads/{})",
                media.original_filename, media.storage_path
            ),
        };
        let _ = write!(
            body,
            r#"<option value="{}"{}>{}</option>"#,
            media.id,
            selected,
            escape_html(&label)
        );
    }
    body
}

fn markdown_alt_text(value: &str) -> String {
    value.replace('\\', "\\\\").replace(']', "\\]")
}

fn workflow_submit_button(post: Option<&PostDetail>) -> String {
    let Some(post) = post else {
        return String::new();
    };
    let (status, label) = match post.status.as_str() {
        "draft" => ("published", "Publish"),
        "published" => ("draft", "Unpublish"),
        _ => return String::new(),
    };
    format!(r#"<button type="submit" name="workflow_status" value="{status}">{label}</button>"#)
}

fn cover_image_html(
    storage_path: Option<&str>,
    alt_text: Option<&str>,
    class_name: &str,
) -> String {
    cover_image_html_with_loading(storage_path, alt_text, class_name, "lazy", None)
}

fn cover_image_html_with_loading(
    storage_path: Option<&str>,
    alt_text: Option<&str>,
    class_name: &str,
    loading: &str,
    fetchpriority: Option<&str>,
) -> String {
    let Some(storage_path) = storage_path else {
        return String::new();
    };
    let fetchpriority = fetchpriority
        .map(|value| format!(r#" fetchpriority="{}""#, escape_html(value)))
        .unwrap_or_default();
    format!(
        r#"<img class="{}" src="/uploads/{}" alt="{}" loading="{}"{}>"#,
        escape_html(class_name),
        escape_html(storage_path),
        escape_html(alt_text.unwrap_or("")),
        escape_html(loading),
        fetchpriority,
    )
}

fn social_image_meta(image_url: Option<&str>, alt_text: Option<&str>) -> String {
    let Some(image_url) = image_url else {
        return String::new();
    };
    let alt_text = alt_text.unwrap_or("");
    format!(
        r#"<meta property="og:image" content="{}">
            <meta property="og:image:alt" content="{}">
            <meta name="twitter:image" content="{}">
            <meta name="twitter:image:alt" content="{}">"#,
        escape_html(image_url),
        escape_html(alt_text),
        escape_html(image_url),
        escape_html(alt_text),
    )
}

fn selected_option(value: &str, label: &str, current: &str) -> String {
    if value == current {
        format!(r#"<option value="{value}" selected>{label}</option>"#)
    } else {
        format!(r#"<option value="{value}">{label}</option>"#)
    }
}

fn format_date(value: DateTime<Utc>) -> String {
    value.format("%Y-%m-%d %H:%M UTC").to_string()
}

fn not_found() -> Response {
    (StatusCode::NOT_FOUND, Html("Not found".to_owned())).into_response()
}

fn server_error(error: impl std::fmt::Debug) -> Response {
    tracing::error!(?error, "request failed");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Html("Something went wrong".to_owned()),
    )
        .into_response()
}
