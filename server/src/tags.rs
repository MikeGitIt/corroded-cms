use std::fmt::Write;

use anyhow::{Result, anyhow, bail};
use axum::{
    Form,
    extract::{Path, State},
    http::HeaderMap,
    response::{Html, IntoResponse, Response},
};
use serde::Deserialize;
use shared::{slugify, validate_slug};
use sqlx::Row;
use uuid::Uuid;

use crate::{
    AppState, auth,
    html::{escape_html, not_found_page, page_end, page_start, redirect, server_error_page},
};

#[derive(Debug)]
pub struct Tag {
    id: Uuid,
    name: String,
    slug: String,
    archived: bool,
}

#[derive(Debug, Deserialize)]
pub struct TagForm {
    #[serde(default)]
    csrf_token: String,
    name: String,
    slug: String,
}

pub async fn admin_list(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if auth::current_admin(&state, &headers).await.is_none() {
        return redirect("/admin/login");
    }

    match list_tags(&state).await {
        Ok(tags) => {
            admin_tags_html(&tags, None, &auth::csrf_input(&state, &headers)).into_response()
        }
        Err(error) => server_error(error),
    }
}

pub async fn admin_create(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<TagForm>,
) -> Response {
    if auth::current_admin(&state, &headers).await.is_none() {
        return redirect("/admin/login");
    }
    let csrf_input = auth::csrf_input(&state, &headers);
    if !auth::verify_csrf(&state, &headers, &form.csrf_token) {
        return auth::csrf_rejection();
    }

    match create_tag(&state, form).await {
        Ok(()) => redirect("/admin/tags"),
        Err(error) => match list_tags(&state).await {
            Ok(tags) => {
                admin_tags_html(&tags, Some(&error.to_string()), &csrf_input).into_response()
            }
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

    match get_tag(&state, id).await {
        Ok(Some(tag)) => {
            let action = format!("/admin/tags/{id}");
            tag_form_html(
                "Edit Tag",
                &action,
                &tag,
                None,
                &auth::csrf_input(&state, &headers),
            )
            .into_response()
        }
        Ok(None) => not_found(),
        Err(error) => server_error(error),
    }
}

pub async fn admin_update(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Form(form): Form<TagForm>,
) -> Response {
    if auth::current_admin(&state, &headers).await.is_none() {
        return redirect("/admin/login");
    }
    let csrf_input = auth::csrf_input(&state, &headers);
    if !auth::verify_csrf(&state, &headers, &form.csrf_token) {
        return auth::csrf_rejection();
    }

    match update_tag(&state, id, form).await {
        Ok(true) => redirect("/admin/tags"),
        Ok(false) => not_found(),
        Err(error) => match get_tag(&state, id).await {
            Ok(Some(tag)) => {
                let action = format!("/admin/tags/{id}");
                tag_form_html(
                    "Edit Tag",
                    &action,
                    &tag,
                    Some(&error.to_string()),
                    &csrf_input,
                )
                .into_response()
            }
            Ok(None) => not_found(),
            Err(_) => server_error(error),
        },
    }
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

    match archive_tag(&state, id).await {
        Ok(()) => redirect("/admin/tags"),
        Err(error) => server_error(error),
    }
}

async fn list_tags(state: &AppState) -> Result<Vec<Tag>> {
    let rows = sqlx::query(
        r#"
        SELECT id, name, slug, archived_at IS NOT NULL AS archived
        FROM tags
        ORDER BY archived_at NULLS FIRST, name ASC
        "#,
    )
    .fetch_all(&state.pool)
    .await?;

    rows.into_iter()
        .map(|row| {
            Ok(Tag {
                id: row.try_get("id")?,
                name: row.try_get("name")?,
                slug: row.try_get("slug")?,
                archived: row.try_get("archived")?,
            })
        })
        .collect()
}

async fn get_tag(state: &AppState, id: Uuid) -> Result<Option<Tag>> {
    let row = sqlx::query(
        r#"
        SELECT id, name, slug, archived_at IS NOT NULL AS archived
        FROM tags
        WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await?;

    row.map(|row| {
        Ok(Tag {
            id: row.try_get("id")?,
            name: row.try_get("name")?,
            slug: row.try_get("slug")?,
            archived: row.try_get("archived")?,
        })
    })
    .transpose()
}

async fn create_tag(state: &AppState, form: TagForm) -> Result<()> {
    let (name, slug) = validate_tag_form(form)?;
    sqlx::query(
        r#"
        INSERT INTO tags (name, slug)
        VALUES ($1, $2)
        "#,
    )
    .bind(name)
    .bind(slug)
    .execute(&state.pool)
    .await
    .map_err(|error| anyhow!("failed to create tag: {error}"))?;
    Ok(())
}

async fn update_tag(state: &AppState, id: Uuid, form: TagForm) -> Result<bool> {
    let (name, slug) = validate_tag_form(form)?;
    let result = sqlx::query(
        r#"
        UPDATE tags
        SET name = $1,
            slug = $2,
            updated_at = now()
        WHERE id = $3
        "#,
    )
    .bind(name)
    .bind(slug)
    .bind(id)
    .execute(&state.pool)
    .await
    .map_err(|error| anyhow!("failed to update tag: {error}"))?;
    Ok(result.rows_affected() == 1)
}

async fn archive_tag(state: &AppState, id: Uuid) -> Result<()> {
    sqlx::query("UPDATE tags SET archived_at = now(), updated_at = now() WHERE id = $1")
        .bind(id)
        .execute(&state.pool)
        .await?;
    Ok(())
}

fn validate_tag_form(form: TagForm) -> Result<(String, String)> {
    let name = form.name.trim().to_owned();
    if name.is_empty() || name.chars().count() > 80 {
        bail!("tag name must be 1 to 80 characters");
    }

    let slug = if form.slug.trim().is_empty() {
        slugify(&name)
    } else {
        slugify(&form.slug)
    };
    validate_slug(&slug, 100).map_err(|error| anyhow!("{error}"))?;
    Ok((name, slug))
}

fn admin_tags_html(tags: &[Tag], error: Option<&str>, csrf_input: &str) -> Html<String> {
    let mut body = page_start("Tags");
    body.push_str(
        r#"
        <section class="admin-dashboard">
            <div>
                <p class="eyebrow">Admin</p>
                <h1>Tags</h1>
            </div>
            <a class="button-link" href="/admin/posts">Posts</a>
        </section>
        "#,
    );

    if let Some(error) = error {
        let _ = write!(body, r#"<p class="form-error">{}</p>"#, escape_html(error));
    }

    let _ = write!(
        body,
        r#"
        <form method="post" action="/admin/tags" class="inline-form">
            {}
            <label>
                <span>Name</span>
                <input name="name" maxlength="80" required>
            </label>
            <label>
                <span>Slug</span>
                <input name="slug" maxlength="100" placeholder="generated from name">
            </label>
            <button type="submit">Create tag</button>
        </form>
        "#,
        csrf_input,
    );

    if tags.is_empty() {
        body.push_str(r#"<p class="empty-state">No tags yet.</p>"#);
    } else {
        body.push_str(r#"<div class="table-wrap"><table class="admin-table"><thead><tr><th>Name</th><th>Slug</th><th>Status</th><th></th></tr></thead><tbody>"#);
        for tag in tags {
            let status = if tag.archived { "archived" } else { "active" };
            let action = if tag.archived {
                String::new()
            } else {
                format!(
                    r#"<form method="post" action="/admin/tags/{}/archive">{}<button type="submit">Archive</button></form>"#,
                    tag.id, csrf_input,
                )
            };
            let _ = write!(
                body,
                r#"<tr><td><a href="/admin/tags/{}/edit">{}</a></td><td>{}</td><td>{}</td><td>{}</td></tr>"#,
                tag.id,
                escape_html(&tag.name),
                escape_html(&tag.slug),
                status,
                action
            );
        }
        body.push_str("</tbody></table></div>");
    }

    body.push_str(&page_end());
    Html(body)
}

fn tag_form_html(
    title: &str,
    action: &str,
    tag: &Tag,
    error: Option<&str>,
    csrf_input: &str,
) -> Html<String> {
    let mut body = page_start(title);
    let status = if tag.archived { "archived" } else { "active" };

    let _ = write!(
        body,
        r#"
        <section class="editor-header">
            <div>
                <p class="eyebrow">Admin</p>
                <h1>{}</h1>
            </div>
            <a href="/admin/tags">Back to tags</a>
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
        <form method="post" action="{}" class="post-form tag-edit-form">
            {}
            <label>
                <span>Name</span>
                <input name="name" value="{}" maxlength="80" required>
            </label>
            <label>
                <span>Slug</span>
                <input name="slug" value="{}" maxlength="100" placeholder="generated from name">
            </label>
            <p class="editor-meta">Status: {}</p>
            <div class="form-actions">
                <button type="submit">Save tag</button>
            </div>
        </form>
        "#,
        escape_html(action),
        csrf_input,
        escape_html(&tag.name),
        escape_html(&tag.slug),
        status,
    );

    body.push_str(&page_end());
    Html(body)
}

fn not_found() -> Response {
    not_found_page()
}

fn server_error(error: impl std::fmt::Debug) -> Response {
    tracing::error!(?error, "request failed");
    server_error_page()
}
