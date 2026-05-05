use std::fmt::Write;

use anyhow::{Context, Result, anyhow, bail};
use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng},
};
use axum::{
    Form,
    extract::State,
    http::{
        HeaderMap, StatusCode,
        header::{COOKIE, LOCATION, SET_COOKIE},
    },
    response::{Html, IntoResponse, Response},
};
use chrono::{DateTime, Duration, Utc};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::html::{escape_html, page_end, page_start, redirect};
use crate::{AppState, config::AppConfig};

const SESSION_COOKIE: &str = "corroded_session";
const SESSION_DAYS: i64 = 14;

#[derive(Debug, Deserialize)]
pub struct LoginForm {
    email: String,
    password: String,
}

#[derive(Debug, Deserialize)]
pub struct AccountForm {
    #[serde(default)]
    csrf_token: String,
    display_name: String,
    current_password: String,
    new_password: String,
    confirm_password: String,
}

#[derive(Debug, Deserialize)]
pub struct CsrfForm {
    #[serde(default)]
    pub csrf_token: String,
}

#[derive(Debug)]
pub struct AdminUser {
    pub id: Uuid,
    pub email: String,
    pub display_name: String,
}

pub async fn create_admin(
    pool: &PgPool,
    email: &str,
    display_name: &str,
    password: &str,
) -> Result<()> {
    let email = normalize_email(email)?;
    let display_name = normalize_display_name(display_name)?;
    validate_password(password)?;
    let password_hash = hash_password(password)?;

    sqlx::query(
        r#"
        INSERT INTO users (email, display_name, password_hash, role)
        VALUES ($1, $2, $3, 'admin')
        "#,
    )
    .bind(&email)
    .bind(&display_name)
    .bind(&password_hash)
    .execute(pool)
    .await
    .with_context(|| format!("failed to create admin user `{email}`"))?;

    Ok(())
}

pub async fn login_page(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if current_admin(&state, &headers).await.is_some() {
        return redirect("/admin");
    }

    login_html(None).into_response()
}

pub async fn login_submit(State(state): State<AppState>, Form(form): Form<LoginForm>) -> Response {
    match verify_login(&state.pool, &form.email, &form.password).await {
        Ok(Some(user_id)) => match create_session(&state.pool, &state.config, user_id).await {
            Ok((token, expires_at)) => {
                let cookie = session_cookie(&state.config, &token, expires_at);
                (
                    StatusCode::SEE_OTHER,
                    [(LOCATION, "/admin"), (SET_COOKIE, cookie.as_str())],
                )
                    .into_response()
            }
            Err(error) => {
                tracing::error!(?error, "failed to create session");
                login_html(Some("Login failed. Try again.")).into_response()
            }
        },
        Ok(None) => login_html(Some("Invalid email or password.")).into_response(),
        Err(error) => {
            tracing::error!(?error, "failed to verify login");
            login_html(Some("Invalid email or password.")).into_response()
        }
    }
}

pub async fn logout_get(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if current_admin(&state, &headers).await.is_some() {
        redirect("/admin")
    } else {
        redirect("/admin/login")
    }
}

pub async fn logout_submit(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<CsrfForm>,
) -> Response {
    if current_admin(&state, &headers).await.is_none() {
        return redirect("/admin/login");
    }
    if !verify_csrf(&state, &headers, &form.csrf_token) {
        return csrf_rejection();
    }

    if let Some(token) = session_token(&headers) {
        let token_hash = session_hash(&state.config, token);
        if let Err(error) = sqlx::query("DELETE FROM sessions WHERE token_hash = $1")
            .bind(token_hash)
            .execute(&state.pool)
            .await
        {
            tracing::warn!(?error, "failed to delete session during logout");
        }
    }

    (
        StatusCode::SEE_OTHER,
        [
            (LOCATION, "/admin/login"),
            (SET_COOKIE, expired_cookie().as_str()),
        ],
    )
        .into_response()
}

pub async fn admin_dashboard(State(state): State<AppState>, headers: HeaderMap) -> Response {
    match current_admin(&state, &headers).await {
        Some(user) => dashboard_html(&user, &csrf_input(&state, &headers)).into_response(),
        None => redirect("/admin/login"),
    }
}

pub async fn account_page(State(state): State<AppState>, headers: HeaderMap) -> Response {
    match current_admin(&state, &headers).await {
        Some(user) => account_html(&user, None, &csrf_input(&state, &headers)).into_response(),
        None => redirect("/admin/login"),
    }
}

pub async fn account_submit(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<AccountForm>,
) -> Response {
    let Some(user) = current_admin(&state, &headers).await else {
        return redirect("/admin/login");
    };
    if !verify_csrf(&state, &headers, &form.csrf_token) {
        return csrf_rejection();
    }

    match update_account(&state, &user, form).await {
        Ok(()) => redirect("/admin/account"),
        Err(error) => account_html(
            &user,
            Some(&error.to_string()),
            &csrf_input(&state, &headers),
        )
        .into_response(),
    }
}

async fn verify_login(pool: &PgPool, email: &str, password: &str) -> Result<Option<Uuid>> {
    let email = normalize_email(email)?;
    let Some(row) = sqlx::query("SELECT id, password_hash FROM users WHERE email = $1")
        .bind(email)
        .fetch_optional(pool)
        .await?
    else {
        return Ok(None);
    };

    let user_id: Uuid = row.try_get("id")?;
    let password_hash: String = row.try_get("password_hash")?;
    let parsed_hash = PasswordHash::new(&password_hash)
        .map_err(|error| anyhow!("stored password hash is invalid: {error}"))?;

    match Argon2::default().verify_password(password.as_bytes(), &parsed_hash) {
        Ok(()) => Ok(Some(user_id)),
        Err(_) => Ok(None),
    }
}

async fn update_account(state: &AppState, user: &AdminUser, form: AccountForm) -> Result<()> {
    let display_name = normalize_display_name(&form.display_name)?;

    if form.new_password.trim().is_empty()
        && form.confirm_password.trim().is_empty()
        && form.current_password.trim().is_empty()
    {
        sqlx::query("UPDATE users SET display_name = $1, updated_at = now() WHERE id = $2")
            .bind(display_name)
            .bind(user.id)
            .execute(&state.pool)
            .await?;
        return Ok(());
    }

    if form.new_password != form.confirm_password {
        bail!("new password confirmation did not match");
    }
    validate_password(&form.new_password)?;

    let row = sqlx::query("SELECT password_hash FROM users WHERE id = $1")
        .bind(user.id)
        .fetch_one(&state.pool)
        .await?;
    let password_hash: String = row.try_get("password_hash")?;
    let parsed_hash = PasswordHash::new(&password_hash)
        .map_err(|error| anyhow!("stored password hash is invalid: {error}"))?;
    if Argon2::default()
        .verify_password(form.current_password.as_bytes(), &parsed_hash)
        .is_err()
    {
        bail!("current password is incorrect");
    }

    let new_hash = hash_password(&form.new_password)?;
    sqlx::query(
        "UPDATE users SET display_name = $1, password_hash = $2, updated_at = now() WHERE id = $3",
    )
    .bind(display_name)
    .bind(new_hash)
    .bind(user.id)
    .execute(&state.pool)
    .await?;

    Ok(())
}

async fn create_session(
    pool: &PgPool,
    config: &AppConfig,
    user_id: Uuid,
) -> Result<(String, DateTime<Utc>)> {
    let token = format!("{}.{}", Uuid::new_v4(), Uuid::new_v4());
    let token_hash = session_hash(config, &token);
    let expires_at = Utc::now() + Duration::days(SESSION_DAYS);

    sqlx::query(
        r#"
        INSERT INTO sessions (user_id, token_hash, expires_at)
        VALUES ($1, $2, $3)
        "#,
    )
    .bind(user_id)
    .bind(token_hash)
    .bind(expires_at)
    .execute(pool)
    .await?;

    Ok((token, expires_at))
}

pub async fn current_admin(state: &AppState, headers: &HeaderMap) -> Option<AdminUser> {
    let token = session_token(headers)?;
    let token_hash = session_hash(&state.config, token);

    let row = sqlx::query(
        r#"
        SELECT users.id, users.email, users.display_name
        FROM sessions
        JOIN users ON users.id = sessions.user_id
        WHERE sessions.token_hash = $1
          AND sessions.expires_at > now()
          AND users.role = 'admin'
        "#,
    )
    .bind(&token_hash)
    .fetch_optional(&state.pool)
    .await
    .ok()??;

    if let Err(error) =
        sqlx::query("UPDATE sessions SET last_seen_at = now() WHERE token_hash = $1")
            .bind(token_hash)
            .execute(&state.pool)
            .await
    {
        tracing::warn!(?error, "failed to update session last_seen_at");
    }

    Some(AdminUser {
        id: row.try_get("id").ok()?,
        email: row.try_get("email").ok()?,
        display_name: row.try_get("display_name").ok()?,
    })
}

pub fn csrf_input(state: &AppState, headers: &HeaderMap) -> String {
    csrf_token(state, headers)
        .map(|token| {
            format!(
                r#"<input type="hidden" name="csrf_token" value="{}">"#,
                escape_html(&token)
            )
        })
        .unwrap_or_default()
}

pub fn verify_csrf(state: &AppState, headers: &HeaderMap, submitted: &str) -> bool {
    !submitted.is_empty()
        && csrf_token(state, headers)
            .map(|expected| expected == submitted)
            .unwrap_or(false)
}

pub fn csrf_rejection() -> Response {
    (StatusCode::BAD_REQUEST, "invalid CSRF token").into_response()
}

fn hash_password(password: &str) -> Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    Ok(Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map_err(|error| anyhow!("failed to hash password: {error}"))?
        .to_string())
}

fn validate_password(password: &str) -> Result<()> {
    let len = password.chars().count();
    if !(12..=128).contains(&len) {
        bail!("password must be 12 to 128 characters");
    }
    Ok(())
}

fn normalize_email(email: &str) -> Result<String> {
    let email = email.trim().to_ascii_lowercase();
    if email.is_empty() || !email.contains('@') || email.len() > 254 {
        bail!("email is invalid");
    }
    Ok(email)
}

fn normalize_display_name(display_name: &str) -> Result<String> {
    let display_name = display_name.trim().to_owned();
    if display_name.is_empty() || display_name.chars().count() > 100 {
        bail!("display name must be 1 to 100 characters");
    }
    Ok(display_name)
}

fn session_hash(config: &AppConfig, token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(config.session_secret.as_bytes());
    hasher.update(b":");
    hasher.update(token.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn csrf_token(state: &AppState, headers: &HeaderMap) -> Option<String> {
    let token = session_token(headers)?;
    let mut hasher = Sha256::new();
    hasher.update(state.config.session_secret.as_bytes());
    hasher.update(b":csrf:");
    hasher.update(token.as_bytes());
    Some(format!("{:x}", hasher.finalize()))
}

fn session_token(headers: &HeaderMap) -> Option<&str> {
    let cookies = headers.get(COOKIE)?.to_str().ok()?;
    cookies.split(';').find_map(|cookie| {
        let (name, value) = cookie.trim().split_once('=')?;
        (name == SESSION_COOKIE && !value.is_empty()).then_some(value)
    })
}

fn session_cookie(config: &AppConfig, token: &str, expires_at: DateTime<Utc>) -> String {
    let mut cookie = format!(
        "{SESSION_COOKIE}={token}; Path=/; HttpOnly; SameSite=Lax; Expires={}",
        expires_at.format("%a, %d %b %Y %H:%M:%S GMT")
    );
    if config.environment.is_production() {
        cookie.push_str("; Secure");
    }
    cookie
}

fn expired_cookie() -> String {
    format!(
        "{SESSION_COOKIE}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0; Expires=Thu, 01 Jan 1970 00:00:00 GMT"
    )
}

fn login_html(error: Option<&str>) -> Html<String> {
    let mut body = page_start("Admin Login");
    body.push_str(
        r#"
        <section class="auth-panel">
            <p class="eyebrow">Admin</p>
            <h1>Sign in</h1>
        "#,
    );

    if let Some(error) = error {
        let _ = write!(body, r#"<p class="form-error">{}</p>"#, escape_html(error));
    }

    body.push_str(
        r#"
            <form method="post" action="/admin/login" class="auth-form">
                <label>
                    <span>Email</span>
                    <input name="email" type="email" autocomplete="email" required>
                </label>
                <label>
                    <span>Password</span>
                    <input name="password" type="password" autocomplete="current-password" required>
                </label>
                <button type="submit">Sign in</button>
            </form>
        </section>
        "#,
    );
    body.push_str(&page_end());
    Html(body)
}

fn dashboard_html(user: &AdminUser, csrf_input: &str) -> Html<String> {
    let mut body = page_start("Admin Dashboard");
    let _ = write!(
        body,
        r#"
        <section class="admin-dashboard">
            <div>
                <p class="eyebrow">Admin</p>
                <h1>Dashboard</h1>
                <p>Signed in as <strong>{}</strong> ({})</p>
            </div>
            <form method="post" action="/admin/logout">
                {}
                <button type="submit">Sign out</button>
            </form>
        </section>
        <section class="admin-grid">
            <article>
                <h2><a href="/admin/posts">Posts</a></h2>
                <p>Create, edit, publish, and archive posts.</p>
            </article>
            <article>
                <h2><a href="/admin/tags">Tags</a></h2>
                <p>Create and archive tags.</p>
            </article>
            <article>
                <h2><a href="/admin/media">Media</a></h2>
                <p>Upload and reuse image assets.</p>
            </article>
            <article>
                <h2><a href="/admin/account">Account</a></h2>
                <p>Update display name or password.</p>
            </article>
        </section>
        "#,
        escape_html(&user.display_name),
        escape_html(&user.email),
        csrf_input,
    );
    body.push_str(&page_end());
    Html(body)
}

fn account_html(user: &AdminUser, error: Option<&str>, csrf_input: &str) -> Html<String> {
    let mut body = page_start("Account Settings");
    body.push_str(
        r#"
        <section class="editor-header">
            <div>
                <p class="eyebrow">Admin</p>
                <h1>Account</h1>
            </div>
            <a href="/admin">Dashboard</a>
        </section>
        "#,
    );

    if let Some(error) = error {
        let _ = write!(body, r#"<p class="form-error">{}</p>"#, escape_html(error));
    }

    let _ = write!(
        body,
        r#"
        <form method="post" action="/admin/account" class="post-form">
            {}
            <label>
                <span>Email</span>
                <input value="{}" disabled>
            </label>
            <label>
                <span>Display name</span>
                <input name="display_name" value="{}" maxlength="100" required>
            </label>
            <label>
                <span>Current password</span>
                <input name="current_password" type="password" autocomplete="current-password">
            </label>
            <label>
                <span>New password</span>
                <input name="new_password" type="password" autocomplete="new-password">
            </label>
            <label>
                <span>Confirm new password</span>
                <input name="confirm_password" type="password" autocomplete="new-password">
            </label>
            <div class="form-actions">
                <button type="submit">Save account</button>
            </div>
        </form>
        "#,
        csrf_input,
        escape_html(&user.email),
        escape_html(&user.display_name),
    );

    body.push_str(&page_end());
    Html(body)
}
