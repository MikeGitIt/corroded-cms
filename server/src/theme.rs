use std::{fmt::Write, sync::OnceLock};

use anyhow::{Context, Result, anyhow};
use axum::{
    Form,
    extract::State,
    http::HeaderMap,
    response::{Html, IntoResponse, Response},
};
use serde::Deserialize;
use sqlx::PgPool;
use tokio::sync::RwLock;

use crate::{
    AppState, auth,
    html::{escape_html, page_end, page_start, redirect, server_error_page},
};

pub trait ThemePlugin: Sync {
    fn id(&self) -> &'static str;
    fn display_name(&self) -> &'static str;
    fn asset_base(&self) -> &'static str;
    fn nav_items(&self) -> &'static [ThemeNavItem];
    fn footer_groups(&self) -> &'static [ThemeFooterGroup];
    fn footer_description(&self) -> &'static str;

    fn page_start(&self, title: &str, extra_head: &str) -> String {
        let title = escape_html(title);
        format!(
            r##"<!DOCTYPE html>
        <html lang="en">
        <head>
            <meta charset="utf-8">
            <meta name="viewport" content="width=device-width, initial-scale=1">
            <title>{title}</title>
            <link rel="icon" type="image/svg+xml" href="{asset_base}/favicon.svg">
            <link rel="alternate" type="application/rss+xml" href="/feed.xml">
            <link rel="stylesheet" href="/pkg/corroded-cms.css">
            {extra_head}
        </head>
        <body>
            <div class="app-shell theme-{theme_id}">
                <a href="#main" class="skip-link">Skip to main content</a>
                {header}
                <main id="main" class="site-main">
        "##,
            asset_base = self.asset_base(),
            theme_id = escape_html(self.id()),
            header = self.header_html(),
        )
    }

    fn page_end(&self) -> String {
        format!(
            r#"</main>
                {}
            </div>
        </body>
        </html>"#,
            self.footer_html()
        )
    }

    fn header_html(&self) -> String {
        let mut nav = String::new();
        for item in self.nav_items() {
            let _ = write!(
                nav,
                r#"<a href="{}">{}</a>"#,
                escape_html(item.href),
                escape_html(item.label)
            );
        }

        format!(
            r#"<header class="site-header">
                    <div class="container site-header__inner">
                        <a class="brand" href="/" aria-label="GigaTier home">
                            <img src="{asset_base}/logo.svg" alt="GigaTier" width="220" height="40">
                        </a>
                        <nav class="site-nav" aria-label="Primary">
                            {nav}
                        </nav>
                        <a class="nav-action" href="/admin">Admin</a>
                    </div>
                </header>"#,
            asset_base = self.asset_base(),
        )
    }

    fn footer_html(&self) -> String {
        let mut groups = String::new();
        for group in self.footer_groups() {
            let mut links = String::new();
            for item in group.links {
                let _ = write!(
                    links,
                    r#"<a href="{}">{}</a>"#,
                    escape_html(item.href),
                    escape_html(item.label)
                );
            }
            let _ = write!(
                groups,
                r#"<div>
                        <h2 class="footer__heading">{}</h2>
                        <div class="footer__links">{}</div>
                    </div>"#,
                escape_html(group.label),
                links
            );
        }

        format!(
            r#"<footer class="footer">
                    <div class="container">
                        <div class="footer__grid">
                            <div class="footer__brand">
                                <a href="/" aria-label="GigaTier home">
                                    <img src="{asset_base}/logo.svg" alt="GigaTier" width="180" height="33">
                                </a>
                                <p>{description}</p>
                            </div>
                            {groups}
                        </div>
                        <div class="footer__bottom">
                            <span>&copy; 2026 GigaTier Technologies.</span>
                            <span>Powered by Corroded CMS.</span>
                        </div>
                    </div>
                </footer>"#,
            asset_base = self.asset_base(),
            description = escape_html(self.footer_description()),
        )
    }
}

#[derive(Clone, Copy)]
pub struct ThemeNavItem {
    pub label: &'static str,
    pub href: &'static str,
}

#[derive(Clone, Copy)]
pub struct ThemeFooterGroup {
    pub label: &'static str,
    pub links: &'static [ThemeNavItem],
}

pub struct GigaTierTheme;

static GIGATIER_THEME: GigaTierTheme = GigaTierTheme;
pub const DEFAULT_THEME_ID: &str = "gigatier";

const GIGATIER_NAV: &[ThemeNavItem] = &[
    ThemeNavItem {
        label: "Home",
        href: "/",
    },
    ThemeNavItem {
        label: "Blog",
        href: "/blog",
    },
    ThemeNavItem {
        label: "RSS",
        href: "/feed.xml",
    },
];

const FOOTER_PRODUCT_LINKS: &[ThemeNavItem] = &[
    ThemeNavItem {
        label: "Velociportr",
        href: "/#solution",
    },
    ThemeNavItem {
        label: "Blog",
        href: "/blog",
    },
    ThemeNavItem {
        label: "RSS",
        href: "/feed.xml",
    },
];

const FOOTER_MANAGE_LINKS: &[ThemeNavItem] = &[
    ThemeNavItem {
        label: "Admin",
        href: "/admin",
    },
    ThemeNavItem {
        label: "Themes",
        href: "/admin/themes",
    },
    ThemeNavItem {
        label: "Media",
        href: "/admin/media",
    },
    ThemeNavItem {
        label: "Posts",
        href: "/admin/posts",
    },
];

const FOOTER_CONNECT_LINKS: &[ThemeNavItem] = &[
    ThemeNavItem {
        label: "Email",
        href: "mailto:mlatham@gigatier.com",
    },
    ThemeNavItem {
        label: "LinkedIn",
        href: "https://linkedin.com/in/gigatier/",
    },
    ThemeNavItem {
        label: "GitHub",
        href: "https://github.com/gigatier",
    },
];

const GIGATIER_FOOTER_GROUPS: &[ThemeFooterGroup] = &[
    ThemeFooterGroup {
        label: "Product",
        links: FOOTER_PRODUCT_LINKS,
    },
    ThemeFooterGroup {
        label: "Manage",
        links: FOOTER_MANAGE_LINKS,
    },
    ThemeFooterGroup {
        label: "Connect",
        links: FOOTER_CONNECT_LINKS,
    },
];

const ACTIVE_THEME_SETTING: &str = "theme.active";
static ACTIVE_THEME_ID: OnceLock<RwLock<String>> = OnceLock::new();

#[derive(Debug, Deserialize)]
pub struct ThemeSettingsForm {
    #[serde(default)]
    csrf_token: String,
    active_theme: String,
}

impl ThemePlugin for GigaTierTheme {
    fn id(&self) -> &'static str {
        "gigatier"
    }

    fn display_name(&self) -> &'static str {
        "GigaTier"
    }

    fn asset_base(&self) -> &'static str {
        "/themes/gigatier"
    }

    fn nav_items(&self) -> &'static [ThemeNavItem] {
        GIGATIER_NAV
    }

    fn footer_groups(&self) -> &'static [ThemeFooterGroup] {
        GIGATIER_FOOTER_GROUPS
    }

    fn footer_description(&self) -> &'static str {
        "Building the future of autonomous code migration. Transpile C/C++ to safe, verified Rust at scale."
    }
}

pub fn default_theme() -> &'static dyn ThemePlugin {
    &GIGATIER_THEME
}

pub fn active_theme() -> &'static dyn ThemePlugin {
    if let Some(lock) = ACTIVE_THEME_ID.get() {
        if let Ok(theme_id) = lock.try_read() {
            if let Some(theme) = theme_by_id(theme_id.as_str()) {
                return theme;
            }
        }
    }

    default_theme()
}

pub fn registered_themes() -> [&'static dyn ThemePlugin; 1] {
    [&GIGATIER_THEME]
}

pub fn theme_by_id(id: &str) -> Option<&'static dyn ThemePlugin> {
    registered_themes()
        .into_iter()
        .find(|theme| theme.id() == id)
}

pub async fn initialize_active_theme(pool: &PgPool, configured_theme_id: &str) -> Result<()> {
    let configured_theme = theme_by_id(configured_theme_id)
        .ok_or_else(|| anyhow!("unsupported THEME value `{configured_theme_id}`"))?;
    ensure_active_theme_setting(pool, configured_theme.id()).await?;
    let stored_theme_id = load_active_theme_id(pool).await?;
    let active_theme = match theme_by_id(&stored_theme_id) {
        Some(theme) => theme,
        None => {
            save_active_theme_id(pool, configured_theme.id()).await?;
            configured_theme
        }
    };
    set_active_theme(active_theme.id()).await;
    Ok(())
}

pub async fn admin_page(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if auth::current_admin(&state, &headers).await.is_none() {
        return redirect("/admin/login");
    }

    match load_active_theme_id(&state.pool).await {
        Ok(active_theme_id) => {
            let csrf_input = auth::csrf_input(&state, &headers);
            themes_html(&active_theme_id, None, &csrf_input).into_response()
        }
        Err(error) => server_error(error),
    }
}

pub async fn admin_update(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<ThemeSettingsForm>,
) -> Response {
    if auth::current_admin(&state, &headers).await.is_none() {
        return redirect("/admin/login");
    }
    let csrf_input = auth::csrf_input(&state, &headers);
    if !auth::verify_csrf(&state, &headers, &form.csrf_token) {
        return auth::csrf_rejection();
    }

    let active_theme = form.active_theme.trim();
    let Some(theme) = theme_by_id(active_theme) else {
        return match load_active_theme_id(&state.pool).await {
            Ok(active_theme_id) => (
                axum::http::StatusCode::BAD_REQUEST,
                themes_html(&active_theme_id, Some("Selected theme is not registered."), &csrf_input),
            )
                .into_response(),
            Err(error) => server_error(error),
        };
    };

    match save_active_theme_id(&state.pool, theme.id()).await {
        Ok(()) => {
            set_active_theme(theme.id()).await;
            redirect("/admin/themes")
        }
        Err(error) => server_error(error),
    }
}

async fn ensure_active_theme_setting(pool: &PgPool, theme_id: &str) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO site_settings (key, value)
        VALUES ($1, $2)
        ON CONFLICT (key) DO NOTHING
        "#,
    )
    .bind(ACTIVE_THEME_SETTING)
    .bind(theme_id)
    .execute(pool)
    .await
    .context("failed to seed active theme setting")?;
    Ok(())
}

async fn load_active_theme_id(pool: &PgPool) -> Result<String> {
    let theme_id = sqlx::query_scalar::<_, String>(
        r#"
        SELECT value
        FROM site_settings
        WHERE key = $1
        "#,
    )
    .bind(ACTIVE_THEME_SETTING)
    .fetch_optional(pool)
    .await
    .context("failed to load active theme setting")?
    .unwrap_or_else(|| DEFAULT_THEME_ID.to_owned());

    Ok(theme_id)
}

async fn save_active_theme_id(pool: &PgPool, theme_id: &str) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO site_settings (key, value, updated_at)
        VALUES ($1, $2, now())
        ON CONFLICT (key)
        DO UPDATE SET value = EXCLUDED.value, updated_at = now()
        "#,
    )
    .bind(ACTIVE_THEME_SETTING)
    .bind(theme_id)
    .execute(pool)
    .await
    .context("failed to save active theme setting")?;
    Ok(())
}

async fn set_active_theme(theme_id: &str) {
    let lock = ACTIVE_THEME_ID.get_or_init(|| RwLock::new(DEFAULT_THEME_ID.to_owned()));
    *lock.write().await = theme_id.to_owned();
}

fn themes_html(active_theme_id: &str, error: Option<&str>, csrf_input: &str) -> Html<String> {
    let mut body = page_start("Themes");
    body.push_str(
        r#"
        <section class="admin-dashboard">
            <div>
                <p class="eyebrow">Admin</p>
                <h1>Themes</h1>
                <p>Registered theme plugins control the public shell, assets, navigation, footer, and public-facing templates.</p>
            </div>
            <a class="button-link" href="/admin">Dashboard</a>
        </section>
        "#,
    );

    if let Some(error) = error {
        let _ = write!(body, r#"<p class="form-error">{}</p>"#, escape_html(error));
    }

    let _ = write!(
        body,
        r#"<form method="post" action="/admin/themes" class="theme-form post-form">{csrf_input}<section class="theme-list" aria-label="Registered themes">"#
    );

    for theme in registered_themes() {
        let checked = (theme.id() == active_theme_id)
            .then_some(" checked")
            .unwrap_or("");
        let active_badge = (theme.id() == active_theme_id)
            .then_some(r#"<span class="badge">Active</span>"#)
            .unwrap_or("");
        let _ = write!(
            body,
            r#"
            <label class="theme-option">
                <input type="radio" name="active_theme" value="{}"{} required>
                <span class="theme-option__body">
                    <span class="theme-option__header">
                        <strong>{}</strong>
                        {}
                    </span>
                    <span class="theme-option__meta">Plugin ID: <code>{}</code></span>
                    <span class="theme-option__meta">Assets: <code>{}</code></span>
                    <span class="theme-option__description">{}</span>
                </span>
            </label>
            "#,
            escape_html(theme.id()),
            checked,
            escape_html(theme.display_name()),
            active_badge,
            escape_html(theme.id()),
            escape_html(theme.asset_base()),
            escape_html(theme.footer_description()),
        );
    }

    body.push_str(
        r#"
            </section>
            <div class="form-actions">
                <button type="submit">Save active theme</button>
            </div>
        </form>
        "#,
    );

    if let Some(theme) = theme_by_id(active_theme_id) {
        body.push_str(&theme_details_html(theme));
    }

    body.push_str(&page_end());
    Html(body)
}

fn theme_details_html(theme: &'static dyn ThemePlugin) -> String {
    let nav_items = theme
        .nav_items()
        .iter()
        .map(|item| format!("{} ({})", item.label, item.href))
        .collect::<Vec<_>>()
        .join(", ");
    let footer_groups = theme
        .footer_groups()
        .iter()
        .map(|group| {
            let links = group
                .links
                .iter()
                .map(|item| item.label)
                .collect::<Vec<_>>()
                .join(", ");
            format!("{}: {}", group.label, links)
        })
        .collect::<Vec<_>>()
        .join("; ");

    format!(
        r#"
        <section class="dashboard-section">
            <div class="section-heading">
                <h2>Active Theme Details</h2>
                <a href="/">View site</a>
            </div>
            <div class="table-wrap">
                <table class="admin-table">
                    <tbody>
                        <tr><th>Name</th><td>{}</td></tr>
                        <tr><th>Plugin ID</th><td><code>{}</code></td></tr>
                        <tr><th>Asset base</th><td><code>{}</code></td></tr>
                        <tr><th>Navigation</th><td>{}</td></tr>
                        <tr><th>Footer groups</th><td>{}</td></tr>
                    </tbody>
                </table>
            </div>
        </section>
        "#,
        escape_html(theme.display_name()),
        escape_html(theme.id()),
        escape_html(theme.asset_base()),
        escape_html(&nav_items),
        escape_html(&footer_groups),
    )
}

fn server_error(error: impl std::fmt::Debug) -> Response {
    tracing::error!(?error, "request failed");
    server_error_page()
}
