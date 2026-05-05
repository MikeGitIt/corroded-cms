use std::fmt::Write;

use crate::html::escape_html;

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

pub fn active_theme_from_env() -> &'static dyn ThemePlugin {
    std::env::var("THEME")
        .ok()
        .and_then(|id| theme_by_id(id.trim()))
        .unwrap_or_else(default_theme)
}

pub fn registered_themes() -> [&'static dyn ThemePlugin; 1] {
    [&GIGATIER_THEME]
}

pub fn theme_by_id(id: &str) -> Option<&'static dyn ThemePlugin> {
    registered_themes()
        .into_iter()
        .find(|theme| theme.id() == id)
}
