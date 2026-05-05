use anyhow::Result;
use sqlx::{PgPool, postgres::PgPoolOptions};
use uuid::Uuid;

async fn test_pool() -> Result<Option<PgPool>> {
    let Ok(database_url) = std::env::var("TEST_DATABASE_URL") else {
        eprintln!("skipping DB integration test: TEST_DATABASE_URL is not set");
        return Ok(None);
    };

    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&database_url)
        .await?;
    Ok(Some(pool))
}

#[tokio::test]
async fn migrations_and_public_visibility_contract() -> Result<()> {
    let Some(pool) = test_pool().await? else {
        return Ok(());
    };

    sqlx::migrate!("../migrations").run(&pool).await?;

    let suffix = Uuid::new_v4().to_string();
    let admin_email = format!("integration-{suffix}@example.test");
    let published_slug = format!("published-{suffix}");
    let draft_slug = format!("draft-{suffix}");

    let mut tx = pool.begin().await?;
    let author_id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO users (email, password_hash, display_name, role)
        VALUES ($1, 'test-hash', 'Integration Admin', 'admin')
        RETURNING id
        "#,
    )
    .bind(&admin_email)
    .fetch_one(&mut *tx)
    .await?;

    sqlx::query(
        r#"
        INSERT INTO posts (title, slug, excerpt, body_markdown, body_html, status, author_id, published_at)
        VALUES ($1, $2, 'Published excerpt', '# Published', '<h1>Published</h1>', 'published', $3, now())
        "#,
    )
    .bind(format!("Published {suffix}"))
    .bind(&published_slug)
    .bind(author_id)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        r#"
        INSERT INTO posts (title, slug, excerpt, body_markdown, body_html, status, author_id)
        VALUES ($1, $2, 'Draft excerpt', '# Draft', '<h1>Draft</h1>', 'draft', $3)
        "#,
    )
    .bind(format!("Draft {suffix}"))
    .bind(&draft_slug)
    .bind(author_id)
    .execute(&mut *tx)
    .await?;

    let visible_published: Option<String> = sqlx::query_scalar(
        r#"
        SELECT slug
        FROM posts
        WHERE slug = $1
          AND status = 'published'
          AND published_at IS NOT NULL
          AND published_at <= now()
        "#,
    )
    .bind(&published_slug)
    .fetch_optional(&mut *tx)
    .await?;
    let visible_draft: Option<String> = sqlx::query_scalar(
        r#"
        SELECT slug
        FROM posts
        WHERE slug = $1
          AND status = 'published'
          AND published_at IS NOT NULL
          AND published_at <= now()
        "#,
    )
    .bind(&draft_slug)
    .fetch_optional(&mut *tx)
    .await?;

    tx.rollback().await?;

    assert_eq!(visible_published.as_deref(), Some(published_slug.as_str()));
    assert!(visible_draft.is_none());

    Ok(())
}
