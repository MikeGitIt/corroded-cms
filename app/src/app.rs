use leptos::prelude::*;
use leptos_meta::{Meta, MetaTags, Stylesheet, Title, provide_meta_context};
use leptos_router::components::Router;

pub fn shell(options: LeptosOptions) -> impl IntoView {
    view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8"/>
                <meta name="viewport" content="width=device-width, initial-scale=1"/>
                <AutoReload options=options.clone()/>
                <HydrationScripts options/>
                <MetaTags/>
                <link rel="icon" type="image/svg+xml" href="/themes/gigatier/favicon.svg"/>
                <link rel="alternate" type="application/rss+xml" href="/feed.xml"/>
            </head>
            <body>
                <App/>
            </body>
        </html>
    }
}

#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();

    view! {
        <Stylesheet id="leptos" href="/pkg/corroded-cms.css"/>
        <Title text="GigaTier Technologies"/>
        <Meta name="description" content="Autonomous C/C++ to Rust transpilation. Verified, validated, delivered."/>

        <Router>
            <div class="app-shell theme-gigatier">
                <a href="#main" class="skip-link">"Skip to main content"</a>
                <header class="site-header">
                    <div class="container site-header__inner">
                        <a class="brand" href="/" aria-label="GigaTier home">
                            <img src="/themes/gigatier/logo.svg" alt="GigaTier" width="220" height="40"/>
                        </a>
                        <nav class="site-nav" aria-label="Primary">
                            <a href="/">"Home"</a>
                            <a href="/blog">"Blog"</a>
                            <a href="/feed.xml">"RSS"</a>
                        </nav>
                        <a class="nav-action" href="/admin">"Admin"</a>
                    </div>
                </header>

                <main id="main" class="site-main">
                    <NotFound/>
                </main>
            </div>
        </Router>
    }
}

#[component]
fn NotFound() -> impl IntoView {
    view! {
        <section class="page-header">
            <p class="eyebrow">"404"</p>
            <h1>"Page not found"</h1>
            <p>"The requested page does not exist."</p>
        </section>
    }
}
