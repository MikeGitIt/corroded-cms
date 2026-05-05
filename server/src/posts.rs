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
    tag_slugs: String,
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

    post_form_html(
        "New Post",
        "/admin/posts",
        None,
        None,
        &auth::csrf_input(&state, &headers),
    )
    .into_response()
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
        Err(error) => post_form_html(
            "New Post",
            "/admin/posts",
            None,
            Some(&error.to_string()),
            &csrf_input,
        )
        .into_response(),
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
        Ok(Some(post)) => {
            let action = format!("/admin/posts/{id}");
            post_form_html(
                "Edit Post",
                &action,
                Some(&post),
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
            Ok(post) => {
                let action = format!("/admin/posts/{id}");
                post_form_html(
                    "Edit Post",
                    &action,
                    post.as_ref(),
                    Some(&error.to_string()),
                    &csrf_input,
                )
                .into_response()
            }
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

    match set_status(&state, id, "archived").await {
        Ok(()) => redirect("/admin/posts"),
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
               COALESCE(array_agg(tags.name ORDER BY tags.name)
                   FILTER (WHERE tags.id IS NOT NULL), ARRAY[]::text[]) AS tags
        FROM posts
        LEFT JOIN post_tags ON post_tags.post_id = posts.id
        LEFT JOIN tags ON tags.id = post_tags.tag_id
        WHERE ($1::text IS NULL OR posts.status = $1)
          AND (
              $2::text IS NULL
              OR posts.title ILIKE '%' || $2 || '%'
              OR posts.slug ILIKE '%' || $2 || '%'
              OR posts.excerpt ILIKE '%' || $2 || '%'
          )
        GROUP BY posts.id
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
    let rows = sqlx::query(
        r#"
        SELECT posts.id,
               posts.title,
               posts.slug,
               posts.excerpt,
               posts.status,
               posts.published_at,
               posts.updated_at,
               COALESCE(array_agg(tags.name ORDER BY tags.name)
                   FILTER (WHERE tags.id IS NOT NULL AND tags.archived_at IS NULL), ARRAY[]::text[]) AS tags
        FROM posts
        LEFT JOIN post_tags ON post_tags.post_id = posts.id
        LEFT JOIN tags ON tags.id = post_tags.tag_id
        WHERE posts.status = 'published'
          AND posts.published_at IS NOT NULL
          AND posts.published_at <= now()
        GROUP BY posts.id
        ORDER BY posts.published_at DESC, posts.created_at DESC
        LIMIT 10
        "#,
    )
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
               COALESCE(array_agg(all_tags.name ORDER BY all_tags.name)
                   FILTER (WHERE all_tags.id IS NOT NULL AND all_tags.archived_at IS NULL), ARRAY[]::text[]) AS tags
        FROM posts
        JOIN post_tags selected_tag ON selected_tag.post_id = posts.id
        LEFT JOIN post_tags all_post_tags ON all_post_tags.post_id = posts.id
        LEFT JOIN tags all_tags ON all_tags.id = all_post_tags.tag_id
        WHERE selected_tag.tag_id = $1
          AND posts.status = 'published'
          AND posts.published_at IS NOT NULL
          AND posts.published_at <= now()
        GROUP BY posts.id
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
               posts.published_at,
               posts.updated_at,
               COALESCE(array_agg(tags.name ORDER BY tags.name)
                   FILTER (WHERE tags.id IS NOT NULL), ARRAY[]::text[]) AS tag_names,
               COALESCE(array_agg(tags.slug ORDER BY tags.name)
                   FILTER (WHERE tags.id IS NOT NULL), ARRAY[]::text[]) AS tag_slugs
        FROM posts
        LEFT JOIN post_tags ON post_tags.post_id = posts.id
        LEFT JOIN tags ON tags.id = post_tags.tag_id
        WHERE posts.id = $1
        GROUP BY posts.id
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
               posts.published_at,
               posts.updated_at,
               COALESCE(array_agg(tags.name ORDER BY tags.name)
                   FILTER (WHERE tags.id IS NOT NULL AND tags.archived_at IS NULL), ARRAY[]::text[]) AS tag_names,
               COALESCE(array_agg(tags.slug ORDER BY tags.name)
                   FILTER (WHERE tags.id IS NOT NULL AND tags.archived_at IS NULL), ARRAY[]::text[]) AS tag_slugs
        FROM posts
        LEFT JOIN post_tags ON post_tags.post_id = posts.id
        LEFT JOIN tags ON tags.id = post_tags.tag_id
        WHERE posts.slug = $1
          AND posts.status = 'published'
          AND posts.published_at IS NOT NULL
          AND posts.published_at <= now()
        GROUP BY posts.id
        "#,
    )
    .bind(slug)
    .fetch_optional(&state.pool)
    .await?;

    row.map(post_detail_from_row).transpose()
}

async fn create_post(state: &AppState, user: &auth::AdminUser, form: PostForm) -> Result<Uuid> {
    let input = validate_post_form(form)?;
    let body_html = render_markdown(&input.body_markdown);
    let published_at: Option<DateTime<Utc>> = (input.status == "published").then(Utc::now);
    let tags = input.tags.clone();

    let id = sqlx::query_scalar::<_, Uuid>(
        r#"
        INSERT INTO posts (title, slug, excerpt, body_markdown, body_html, status, author_id, published_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
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
            published_at = CASE
                WHEN $6 = 'published' AND published_at IS NULL THEN now()
                ELSE published_at
            END,
            updated_at = now()
        WHERE id = $7
        "#,
    )
    .bind(input.title)
    .bind(input.slug)
    .bind(input.excerpt)
    .bind(input.body_markdown)
    .bind(body_html)
    .bind(input.status)
    .bind(id)
    .execute(&state.pool)
    .await?;

    sync_post_tags(state, id, &tags).await?;
    Ok(())
}

async fn set_status(state: &AppState, id: Uuid, status: &str) -> Result<()> {
    sqlx::query("UPDATE posts SET status = $1, updated_at = now() WHERE id = $2")
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

    let status = form.status.trim().to_owned();
    if !matches!(status.as_str(), "draft" | "published" | "archived") {
        bail!("status is invalid");
    }

    let tags = parse_tags(&form.tag_slugs)?;

    Ok(ValidPostForm {
        title,
        slug,
        excerpt,
        body_markdown: form.body_markdown,
        status,
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
    error: Option<&str>,
    csrf_input: &str,
) -> Html<String> {
    let mut body = page_start(title);
    let default_title = post.map(|p| p.title.as_str()).unwrap_or("");
    let default_slug = post.map(|p| p.slug.as_str()).unwrap_or("");
    let default_excerpt = post.map(|p| p.excerpt.as_str()).unwrap_or("");
    let default_body = post.map(|p| p.body_markdown.as_str()).unwrap_or("");
    let default_status = post.map(|p| p.status.as_str()).unwrap_or("draft");
    let default_tags = post.map(|p| p.tag_slugs.join(", ")).unwrap_or_default();

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
                <span>Tags</span>
                <input name="tag_slugs" value="{}" placeholder="rust, cms, release">
            </label>
            <label>
                <span>Markdown</span>
                <textarea name="body_markdown" rows="18">{}</textarea>
            </label>
            <div class="form-actions">
                <button type="submit">Save</button>
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
        escape_html(&default_tags),
        escape_html(default_body),
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
                    <p>{}</p>
                    <h2><a href="/blog/{}">{}</a></h2>
                    <p>{}</p>
                    {}
                </article>
                "#,
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
    let canonical = format!("{}/blog/{}", state.config.base_url, post.slug);
    let mut body = format!(
        r#"<!DOCTYPE html>
        <html lang="en">
        <head>
            <meta charset="utf-8">
            <meta name="viewport" content="width=device-width, initial-scale=1">
            <title>{}</title>
            <meta name="description" content="{}">
            <link rel="canonical" href="{}">
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
        escape_html(&post.title),
        escape_html(&post.excerpt),
        escape_html(&canonical)
    );

    let _ = write!(
        body,
        r#"
        <article class="post-detail">
            <p class="eyebrow">{}</p>
            <h1>{}</h1>
            {}
            <div class="post-body">{}</div>
        </article>
        "#,
        post.published_at
            .map(format_date)
            .unwrap_or_else(|| "Unpublished".to_owned()),
        escape_html(&post.title),
        tag_links_with_slugs(&post.tag_names, &post.tag_slugs),
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
                <p>{}</p>
                <h2><a href="/blog/{}">{}</a></h2>
                <p>{}</p>
                {}
            </article>
            "#,
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
