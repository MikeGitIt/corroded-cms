# Corroded CMS

Corroded CMS is a Rust-native blog CMS built with Leptos SSR, Axum, PostgreSQL, and SQLx. The MVP includes admin authentication, post drafting and publishing, tags, local image uploads, RSS, sitemap generation, CSRF protection, and security response headers. The default site theme is the built-in GigaTier theme plugin.

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

Apply database migrations without starting the web server:

```bash
cargo run -p corroded-cms -- migrate
```

For the same local PostgreSQL defaults used by `just restart-local`:

```bash
just migrate-local
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

Start the rebuilt server in the background and return to the shell:

```bash
just restart-bg
```

For the common local PostgreSQL setup using your macOS/Linux username on `127.0.0.1:5432`:

```bash
just restart-local
```

The detached local variant writes `/private/tmp/corroded-cms-3000.pid` and `/private/tmp/corroded-cms-3000.log` by default:

```bash
just restart-local-bg
```

Override local defaults with `CORRODED_CMS_DB_USER`, `CORRODED_CMS_DATABASE_URL`, `CORRODED_CMS_PORT`, or the matching `CORRODED_CMS_*` config variable.

## Common Commands

```bash
just check       # cargo check --workspace
just test        # cargo test --workspace
just test-db-local # DB integration test with local PostgreSQL defaults
just smoke       # endpoint smoke test against a running local server
just migrate     # run pending database migrations
just migrate-local # migrations with local PostgreSQL defaults
cargo leptos build
just restart     # rebuild, stop the local app server on PORT, and start it again
just restart-bg  # same restart flow, but detach the server and write pid/log files
just restart-local # same restart flow with local PostgreSQL defaults
just restart-local-bg # detached restart with local PostgreSQL defaults
```

DB integration tests are opt-in so the default test suite does not mutate an arbitrary local database:

```bash
TEST_DATABASE_URL='postgres://user@127.0.0.1:5432/corroded_cms_test' \
  cargo test -p corroded-cms --test db_integration
```

For the local PostgreSQL defaults:

```bash
just test-db-local
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

## Backups

Create a PostgreSQL custom-format dump and uploads archive:

```bash
DATABASE_URL='postgres://corroded:corroded@127.0.0.1:5432/corroded_cms' \
UPLOAD_DIR='uploads' \
BACKUP_DIR='backups' \
scripts/backup.sh
```

Restore into a target database and upload directory:

```bash
DATABASE_URL='postgres://corroded:corroded@127.0.0.1:5432/corroded_cms' \
UPLOAD_DIR='uploads' \
DB_BACKUP='backups/corroded-cms-db-YYYYMMDDTHHMMSSZ.dump' \
UPLOADS_BACKUP='backups/corroded-cms-uploads-YYYYMMDDTHHMMSSZ.tar.gz' \
scripts/restore.sh
```

`scripts/restore.sh` runs `pg_restore --clean --if-exists`; point it at the intended restore database, not a production database, unless replacing that database is deliberate.

Run the disposable backup/restore rehearsal:

```bash
just rehearse-backup-restore
```

The rehearsal starts a temporary PostgreSQL 17 container, seeds a marker row and upload fixture, runs `scripts/backup.sh`, restores into a second database in the same temporary container, verifies the restored database and upload archive, then removes the container. It automatically uses `psql-17`, `pg_dump-17`, and `pg_restore-17` when unversioned client commands are not installed.

## Configuration

Required environment variables are documented in `.env.example`:

- `DATABASE_URL`
- `BASE_URL`
- `SESSION_SECRET`
- `UPLOAD_DIR`
- `ENVIRONMENT`
- `SITE_NAME`
- `SITE_DESCRIPTION`

Optional runtime variables include `THEME`, `RUST_LOG`, `HOST`, `PORT`, and `MAX_UPLOAD_BYTES`.

`THEME` selects the active theme plugin. The bundled default is `gigatier`; new themes should register through the server theme plugin boundary instead of hard-coding alternate page shells.

## Repository Layout

- `app/` - Leptos app shell and CSS
- `server/` - Axum server, admin/public routes, auth, feeds, uploads
- `shared/` - shared validation and Markdown rendering logic
- `migrations/` - SQLx PostgreSQL migrations
- `scripts/` - local verification scripts
- `SPECS/` - product requirements and implementation plan

## License

MIT. See `LICENSE`.
