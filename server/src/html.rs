use axum::{
    http::{HeaderValue, StatusCode, header::LOCATION},
    response::{Html, IntoResponse, Response},
};

pub fn page_start(title: &str) -> String {
    page_start_with_head(title, "")
}

pub fn page_start_with_head(title: &str, extra_head: &str) -> String {
    crate::theme::active_theme().page_start(title, extra_head)
}

pub fn page_end() -> String {
    crate::theme::active_theme().page_end()
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
