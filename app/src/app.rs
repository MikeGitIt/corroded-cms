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
        <Title text="Corroded CMS"/>
        <Meta name="description" content="A Rust-native blog CMS."/>

        <Router>
            <div class="app-shell">
                <header class="site-header">
                    <a class="brand" href="/">"Corroded CMS"</a>
                    <nav class="site-nav" aria-label="Primary">
                        <a href="/">"Home"</a>
                        <a href="/blog">"Blog"</a>
                        <a href="/admin">"Admin"</a>
                    </nav>
                </header>

                <main class="site-main">
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
