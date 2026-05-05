use axum::{
    http::{HeaderValue, StatusCode, header::LOCATION},
    response::{Html, IntoResponse, Response},
};

pub fn page_start(title: &str) -> String {
    format!(
        r#"<!DOCTYPE html>
        <html lang="en">
        <head>
            <meta charset="utf-8">
            <meta name="viewport" content="width=device-width, initial-scale=1">
            <title>{}</title>
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
        escape_html(title)
    )
}

pub fn page_end() -> String {
    "</main></div></body></html>".to_owned()
}

pub fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

pub fn redirect(location: impl AsRef<str>) -> Response {
    let location =
        HeaderValue::from_str(location.as_ref()).unwrap_or_else(|_| HeaderValue::from_static("/"));
    (StatusCode::SEE_OTHER, [(LOCATION, location)]).into_response()
}

pub fn not_found_page() -> Response {
    (
        StatusCode::NOT_FOUND,
        Html(error_page(
            "Page Not Found",
            "The page you requested does not exist.",
        )),
    )
        .into_response()
}

pub fn server_error_page() -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Html(error_page(
            "Something Went Wrong",
            "The request could not be completed.",
        )),
    )
        .into_response()
}

fn error_page(title: &str, message: &str) -> String {
    let mut body = page_start(title);
    body.push_str(r#"<section class="page-header">"#);
    body.push_str(r#"<p class="eyebrow">Error</p>"#);
    body.push_str(&format!(r#"<h1>{}</h1>"#, escape_html(title)));
    body.push_str(&format!(r#"<p>{}</p>"#, escape_html(message)));
    body.push_str(r#"<p><a class="button-link" href="/">Go home</a></p>"#);
    body.push_str("</section>");
    body.push_str(&page_end());
    body
}
