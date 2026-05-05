mod auth;
mod cli;
mod config;
mod feeds;
mod html;
mod media;
mod posts;
mod tags;
mod theme;

use std::{sync::Arc, time::Instant};

use anyhow::{Context, Result};
use app::app::shell;
use axum::{
    Router,
    body::Body,
    extract::{DefaultBodyLimit, FromRef, State},
    http::{
        HeaderValue, Request, StatusCode,
        header::{CACHE_CONTROL, CONTENT_SECURITY_POLICY, HeaderName, X_CONTENT_TYPE_OPTIONS},
    },
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use cli::{Cli, Command};
use config::AppConfig;
use leptos::logging::log;
use leptos::prelude::*;
use sqlx::{PgPool, postgres::PgPoolOptions};
use tower_http::{services::ServeDir, trace::TraceLayer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use uuid::Uuid;

#[derive(Clone)]
pub struct AppState {
    pool: PgPool,
    config: Arc<AppConfig>,
    leptos_options: LeptosOptions,
    login_rate_limiter: auth::LoginRateLimiter,
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
        theme = %config.theme,
        theme_name = %theme::theme_by_id(&config.theme).map(|theme| theme.display_name()).unwrap_or("unknown"),
        host = %config.host,
        port = config.port,
        max_upload_bytes = config.max_upload_bytes,
        session_secret_bytes = config.session_secret.len(),
        upload_dir = ?config.upload_dir,
        "loaded configuration"
    );

    let pool = connect_database(&config).await?;

    match cli.command.unwrap_or(Command::Serve) {
        Command::Serve => {
            run_migrations(&pool).await?;
            serve(config, pool).await
        }
        Command::Migrate => {
            run_migrations(&pool).await?;
            println!("Database migrations applied.");
            Ok(())
        }
        Command::CreateAdmin {
            email,
            display_name,
            password,
        } => {
            run_migrations(&pool).await?;
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
    let mut leptos_options = conf.leptos_options;
    let addr = format!("{}:{}", config.host, config.port)
        .parse()
        .with_context(|| format!("failed to parse bind address {}:{}", config.host, config.port))?;
    leptos_options.site_addr = addr;

    let state = AppState {
        pool,
        config: Arc::new(config.clone()),
        leptos_options: leptos_options.clone(),
        login_rate_limiter: auth::LoginRateLimiter::default(),
    };
    let max_body_bytes = usize::try_from(config.max_upload_bytes).unwrap_or(usize::MAX);
    let uploads = ServeDir::new(config.upload_dir.clone());
    let app = Router::new()
        .route("/", get(posts::home))
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
        .route(
            "/admin/logout",
            get(auth::logout_get).post(auth::logout_submit),
        )
        .route("/admin", get(auth::admin_dashboard))
        .route(
            "/admin/account",
            get(auth::account_page).post(auth::account_submit),
        )
        .route(
            "/admin/media",
            get(media::admin_list).post(media::admin_upload),
        )
        .route("/admin/media/{id}", post(media::admin_update))
        .route(
            "/admin/posts",
            get(posts::admin_list).post(posts::admin_create),
        )
        .route("/admin/posts/new", get(posts::admin_new))
        .route("/admin/posts/preview", post(posts::admin_preview))
        .route("/admin/posts/{id}", post(posts::admin_update))
        .route("/admin/posts/{id}/edit", get(posts::admin_edit))
        .route("/admin/posts/{id}/archive", post(posts::admin_archive))
        .route("/admin/posts/{id}/publish", post(posts::admin_publish))
        .route("/admin/posts/{id}/unpublish", post(posts::admin_unpublish))
        .route(
            "/admin/tags",
            get(tags::admin_list).post(tags::admin_create),
        )
        .route("/admin/tags/{id}", post(tags::admin_update))
        .route("/admin/tags/{id}/edit", get(tags::admin_edit))
        .route("/admin/tags/{id}/archive", post(tags::admin_archive))
        .nest_service("/uploads", uploads)
        .fallback(leptos_axum::file_and_error_handler::<AppState, _>(shell))
        .layer(DefaultBodyLimit::max(max_body_bytes))
        .layer(middleware::from_fn(response_headers))
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
    PgPoolOptions::new()
        .max_connections(5)
        .connect(&config.database_url)
        .await
        .context("failed to connect to PostgreSQL")
}

async fn run_migrations(pool: &PgPool) -> Result<()> {
    sqlx::migrate!("../migrations")
        .run(pool)
        .await
        .context("failed to run database migrations")?;
    Ok(())
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

async fn response_headers(request: Request<Body>, next: Next) -> Response {
    let is_upload = request.uri().path().starts_with("/uploads/");
    let method = request.method().clone();
    let path = request.uri().path().to_owned();
    let allow_preview_frame = path == "/admin/posts/preview";
    let request_id = request_id(&request);
    let started = Instant::now();
    let mut response = next.run(request).await;
    let status = response.status();
    let duration_ms = started.elapsed().as_millis();
    let should_cache_upload = is_upload && response.status().is_success();
    let headers = response.headers_mut();

    if let Ok(value) = HeaderValue::from_str(&request_id) {
        headers.insert(HeaderName::from_static("x-request-id"), value);
    }
    let content_security_policy = if allow_preview_frame {
        "default-src 'self'; base-uri 'self'; frame-ancestors 'self'; object-src 'none'; form-action 'self'; img-src 'self' data:; script-src 'self' 'wasm-unsafe-eval'; style-src 'self' 'unsafe-inline'; connect-src 'self' ws: wss:"
    } else {
        "default-src 'self'; base-uri 'self'; frame-ancestors 'none'; object-src 'none'; form-action 'self'; img-src 'self' data:; script-src 'self' 'wasm-unsafe-eval'; style-src 'self' 'unsafe-inline'; connect-src 'self' ws: wss:"
    };
    headers.insert(
        CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(content_security_policy),
    );
    headers.insert(X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    headers.insert(
        HeaderName::from_static("referrer-policy"),
        HeaderValue::from_static("strict-origin-when-cross-origin"),
    );
    headers.insert(
        HeaderName::from_static("permissions-policy"),
        HeaderValue::from_static("camera=(), microphone=(), geolocation=(), payment=()"),
    );

    if should_cache_upload {
        headers.insert(
            CACHE_CONTROL,
            HeaderValue::from_static("public, max-age=31536000, immutable"),
        );
    }

    tracing::info!(
        request_id = %request_id,
        method = %method,
        path = %path,
        status = status.as_u16(),
        duration_ms,
        "request completed"
    );

    response
}

fn request_id(request: &Request<Body>) -> String {
    request
        .headers()
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty() && value.len() <= 128)
        .map(str::to_owned)
        .unwrap_or_else(|| Uuid::new_v4().to_string())
}

fn init_tracing() {
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "corroded_cms=info,server=info,tower_http=info".into());

    tracing_subscriber::registry()
        .with(env_filter)
        .with(tracing_subscriber::fmt::layer())
        .init();
}
