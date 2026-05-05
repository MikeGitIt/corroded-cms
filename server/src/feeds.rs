use axum::{
    extract::State,
    http::{StatusCode, header::CONTENT_TYPE},
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Utc};
use sqlx::Row;

use crate::{
    AppState,
    html::{redirect, server_error_page},
};

struct FeedPost {
    title: String,
    slug: String,
    excerpt: String,
    published_at: DateTime<Utc>,
}

pub async fn rss(State(state): State<AppState>) -> Response {
    match list_feed_posts(&state).await {
        Ok(posts) => {
            let xml = rss_xml(&state, &posts);
            (
                StatusCode::OK,
                [(CONTENT_TYPE, "application/rss+xml; charset=utf-8")],
                xml,
            )
                .into_response()
        }
        Err(error) => server_error(error),
    }
}

pub async fn rss_redirect() -> Response {
    redirect("/feed.xml")
}

pub async fn sitemap(State(state): State<AppState>) -> Response {
    match sitemap_urls(&state).await {
        Ok(urls) => {
            let xml = sitemap_xml(&urls);
            (
                StatusCode::OK,
                [(CONTENT_TYPE, "application/xml; charset=utf-8")],
                xml,
            )
                .into_response()
        }
        Err(error) => server_error(error),
    }
}

async fn list_feed_posts(state: &AppState) -> anyhow::Result<Vec<FeedPost>> {
    let rows = sqlx::query(
        r#"
        SELECT title, slug, excerpt, published_at
        FROM posts
        WHERE status = 'published'
          AND published_at IS NOT NULL
          AND published_at <= now()
        ORDER BY published_at DESC, created_at DESC
        LIMIT 20
        "#,
    )
    .fetch_all(&state.pool)
    .await?;

    rows.into_iter()
        .map(|row| {
            Ok(FeedPost {
                title: row.try_get("title")?,
                slug: row.try_get("slug")?,
                excerpt: row.try_get("excerpt")?,
                published_at: row.try_get("published_at")?,
            })
        })
        .collect()
}

async fn sitemap_urls(state: &AppState) -> anyhow::Result<Vec<(String, Option<DateTime<Utc>>)>> {
    let mut urls = vec![
        (state.config.base_url.clone(), None),
        (format!("{}/blog", state.config.base_url), None),
    ];

    let posts = sqlx::query(
        r#"
        SELECT slug, updated_at
        FROM posts
        WHERE status = 'published'
          AND published_at IS NOT NULL
          AND published_at <= now()
        ORDER BY published_at DESC
        "#,
    )
    .fetch_all(&state.pool)
    .await?;

    for row in posts {
        let slug: String = row.try_get("slug")?;
        let updated_at: DateTime<Utc> = row.try_get("updated_at")?;
        urls.push((
            format!("{}/blog/{slug}", state.config.base_url),
            Some(updated_at),
        ));
    }

    let tags = sqlx::query(
        r#"
        SELECT DISTINCT tags.slug
        FROM tags
        JOIN post_tags ON post_tags.tag_id = tags.id
        JOIN posts ON posts.id = post_tags.post_id
        WHERE tags.archived_at IS NULL
          AND posts.status = 'published'
          AND posts.published_at IS NOT NULL
          AND posts.published_at <= now()
        ORDER BY tags.slug
        "#,
    )
    .fetch_all(&state.pool)
    .await?;

    for row in tags {
        let slug: String = row.try_get("slug")?;
        urls.push((format!("{}/tags/{slug}", state.config.base_url), None));
    }

    Ok(urls)
}

fn rss_xml(state: &AppState, posts: &[FeedPost]) -> String {
    let mut xml = String::from(r#"<?xml version="1.0" encoding="UTF-8"?>"#);
    xml.push_str(r#"<rss version="2.0"><channel>"#);
    xml.push_str(&format!(
        "<title>{}</title><link>{}</link><description>{}</description>",
        xml_escape(&state.config.site_name),
        xml_escape(&state.config.base_url),
        xml_escape(&state.config.site_description)
    ));

    for post in posts {
        let link = format!("{}/blog/{}", state.config.base_url, post.slug);
        xml.push_str("<item>");
        xml.push_str(&format!(
            "<title>{}</title><link>{}</link><description>{}</description><pubDate>{}</pubDate><guid>{}</guid>",
            xml_escape(&post.title),
            xml_escape(&link),
            xml_escape(&post.excerpt),
            post.published_at.to_rfc2822(),
            xml_escape(&link)
        ));
        xml.push_str("</item>");
    }

    xml.push_str("</channel></rss>");
    xml
}

fn sitemap_xml(urls: &[(String, Option<DateTime<Utc>>)]) -> String {
    let mut xml = String::from(
        r#"<?xml version="1.0" encoding="UTF-8"?><urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">"#,
    );

    for (loc, updated_at) in urls {
        xml.push_str("<url>");
        xml.push_str(&format!("<loc>{}</loc>", xml_escape(loc)));
        if let Some(updated_at) = updated_at {
            xml.push_str(&format!("<lastmod>{}</lastmod>", updated_at.date_naive()));
        }
        xml.push_str("</url>");
    }

    xml.push_str("</urlset>");
    xml
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn server_error(error: impl std::fmt::Debug) -> Response {
    tracing::error!(?error, "request failed");
    server_error_page()
}
