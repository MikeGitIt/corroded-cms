mod auth;
mod cli;
mod config;
mod feeds;
mod html;
mod media;
mod posts;
mod tags;

use std::sync::Arc;

use anyhow::{Context, Result};
use app::app::{App, shell};
use axum::{
    Router,
    extract::{DefaultBodyLimit, FromRef, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use cli::{Cli, Command};
use config::AppConfig;
use leptos::logging::log;
use leptos::prelude::*;
use leptos_axum::{LeptosRoutes, generate_route_list};
use sqlx::{PgPool, postgres::PgPoolOptions};
use tower_http::{services::ServeDir, trace::TraceLayer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Clone)]
pub struct AppState {
    pool: PgPool,
    config: Arc<AppConfig>,
    leptos_options: LeptosOptions,
}

impl FromRef<AppState> for LeptosOptions {
    fn from_ref(state: &AppState) -> Self {
        state.leptos_options.clone()
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();

    let cli = Cli::parse_args();
    let config = AppConfig::from_env()?;
    tracing::info!(
        base_url = %config.base_url,
        environment = ?config.environment,
        site_name = %config.site_name,
        site_description = %config.site_description,
        max_upload_bytes = config.max_upload_bytes,
        session_secret_bytes = config.session_secret.len(),
        upload_dir = ?config.upload_dir,
        "loaded configuration"
    );

    let pool = connect_database(&config).await?;

    match cli.command.unwrap_or(Command::Serve) {
        Command::Serve => serve(config, pool).await,
        Command::CreateAdmin {
            email,
            display_name,
            password,
        } => {
            cli::create_admin(&pool, &email, display_name.as_deref(), password).await?;
            println!("Admin user created: {email}");
            Ok(())
        }
    }
}

async fn serve(config: AppConfig, pool: PgPool) -> Result<()> {
    tokio::fs::create_dir_all(&config.upload_dir)
        .await
        .with_context(|| format!("failed to create upload directory {:?}", config.upload_dir))?;

    let conf = get_configuration(None).context("failed to read Leptos configuration")?;
    let leptos_options = conf.leptos_options;
    let addr = leptos_options.site_addr;
    let routes = generate_route_list(App);

    let state = AppState {
        pool,
        config: Arc::new(config.clone()),
        leptos_options: leptos_options.clone(),
    };
    let max_body_bytes = usize::try_from(config.max_upload_bytes).unwrap_or(usize::MAX);
    let uploads = ServeDir::new(config.upload_dir.clone());
    let app = Router::new()
        .route("/healthz", get(healthz))
        .route("/feed.xml", get(feeds::rss))
        .route("/rss.xml", get(feeds::rss_redirect))
        .route("/sitemap.xml", get(feeds::sitemap))
        .route("/blog", get(posts::blog_index))
        .route("/blog/{slug}", get(posts::blog_detail))
        .route("/tags/{slug}", get(posts::tag_detail))
        .route(
            "/admin/login",
            get(auth::login_page).post(auth::login_submit),
        )
        .route("/admin/logout", get(auth::logout).post(auth::logout))
        .route("/admin", get(auth::admin_dashboard))
        .route(
            "/admin/account",
            get(auth::account_page).post(auth::account_submit),
        )
        .route(
            "/admin/media",
            get(media::admin_list).post(media::admin_upload),
        )
        .route(
            "/admin/posts",
            get(posts::admin_list).post(posts::admin_create),
        )
        .route("/admin/posts/new", get(posts::admin_new))
        .route("/admin/posts/{id}", post(posts::admin_update))
        .route("/admin/posts/{id}/edit", get(posts::admin_edit))
        .route("/admin/posts/{id}/archive", post(posts::admin_archive))
        .route(
            "/admin/tags",
            get(tags::admin_list).post(tags::admin_create),
        )
        .route("/admin/tags/{id}/archive", post(tags::admin_archive))
        .nest_service("/uploads", uploads)
        .leptos_routes(&state, routes, {
            let leptos_options = leptos_options.clone();
            move || shell(leptos_options.clone())
        })
        .fallback(leptos_axum::file_and_error_handler::<AppState, _>(shell))
        .layer(DefaultBodyLimit::max(max_body_bytes))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    log!("listening on http://{addr}");
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .with_context(|| format!("failed to bind {addr}"))?;
    axum::serve(listener, app.into_make_service())
        .await
        .context("server failed")?;

    Ok(())
}

async fn connect_database(config: &AppConfig) -> Result<PgPool> {
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&config.database_url)
        .await
        .context("failed to connect to PostgreSQL")?;

    sqlx::migrate!("../migrations")
        .run(&pool)
        .await
        .context("failed to run database migrations")?;

    Ok(pool)
}

async fn healthz(State(state): State<AppState>) -> Response {
    match sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(&state.pool)
        .await
    {
        Ok(1) => StatusCode::NO_CONTENT.into_response(),
        Ok(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            "database health check returned an unexpected value",
        )
            .into_response(),
        Err(_) => (StatusCode::SERVICE_UNAVAILABLE, "database unavailable").into_response(),
    }
}

fn init_tracing() {
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "corroded_cms=info,server=info,tower_http=info".into());

    tracing_subscriber::registry()
        .with(env_filter)
        .with(tracing_subscriber::fmt::layer())
        .init();
}
