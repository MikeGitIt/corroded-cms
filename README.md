# Corroded CMS

Corroded CMS is a Rust-native blog CMS built with Leptos SSR, Axum, PostgreSQL, and SQLx. The MVP includes admin authentication, post drafting and publishing, tags, local image uploads, RSS, sitemap generation, CSRF protection, and security response headers.

## Prerequisites

- Rust nightly with `wasm32-unknown-unknown`
- `cargo-leptos`
- PostgreSQL 17, either local or via Docker Compose
- `just` for the bundled task shortcuts

Install the Rust-side tools:

```bash
rustup target add wasm32-unknown-unknown
cargo install cargo-leptos
```

## Local Setup

Create local configuration:

```bash
cp .env.example .env
```

Start PostgreSQL with Docker Compose:

```bash
just db-up
```

Create an admin user:

```bash
CORRODED_CMS_ADMIN_PASSWORD='TemporaryPass123!' \
  cargo run -p corroded-cms -- create-admin \
  --email admin@example.com \
  --display-name Admin
```

Run the development server:

```bash
just dev
```

The app listens on `http://127.0.0.1:3000` by default.

Restart the local development server after a build:

```bash
just restart
```

For the common local PostgreSQL setup using your macOS/Linux username on `127.0.0.1:5432`:

```bash
just restart-local
```

Override local defaults with `CORRODED_CMS_DB_USER`, `CORRODED_CMS_DATABASE_URL`, `CORRODED_CMS_PORT`, or the matching `CORRODED_CMS_*` config variable.

## Common Commands

```bash
just check       # cargo check --workspace
just test        # cargo test --workspace
just smoke       # endpoint smoke test against a running local server
cargo leptos build
just restart     # rebuild, stop the local app server on PORT, and start it again
just restart-local # same restart flow with local PostgreSQL defaults
```

## Docker

Build the production image:

```bash
docker build -t corroded-cms .
```

Run the app with PostgreSQL through Compose:

```bash
docker compose --profile app up --build
```

The Compose app service binds `0.0.0.0:3000` inside the container and publishes `http://127.0.0.1:3000`. Uploaded files are stored in the `uploads-data` volume.

## Configuration

Required environment variables are documented in `.env.example`:

- `DATABASE_URL`
- `BASE_URL`
- `SESSION_SECRET`
- `UPLOAD_DIR`
- `ENVIRONMENT`
- `SITE_NAME`
- `SITE_DESCRIPTION`

Optional runtime variables include `RUST_LOG`, `HOST`, `PORT`, and `MAX_UPLOAD_BYTES`.

## Repository Layout

- `app/` - Leptos app shell and CSS
- `server/` - Axum server, admin/public routes, auth, feeds, uploads
- `shared/` - shared validation and Markdown rendering logic
- `migrations/` - SQLx PostgreSQL migrations
- `scripts/` - local verification scripts
- `SPECS/` - product requirements and implementation plan

## License

MIT. See `LICENSE`.
