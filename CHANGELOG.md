# Changelog

All notable changes to Corroded CMS will be documented in this file.

## 0.1.0 - 2026-05-05

Initial MVP release.

### Added

- Leptos SSR and Axum application shell.
- PostgreSQL schema migrations for users, sessions, posts, tags, media assets, and post tags.
- Admin authentication, session cookies, CSRF protection, account settings, and first-admin CLI creation.
- Admin post creation, editing, archive, publish, unpublish, scheduling field storage, tag assignment, and cover image selection.
- Server-rendered Markdown preview and public Markdown rendering through the same sanitization path.
- Public home, blog index, post detail pages, and tag archive pages.
- Local image uploads with MIME sniffing, metadata capture, alt text editing, snippet tools, and immutable upload cache headers.
- RSS feed, RSS redirect, sitemap generation, canonical URLs, and social metadata.
- Admin dashboard, media library, tag management, post filters, request IDs, custom error pages, and login rate limiting.
- Dockerfile, Docker Compose app/Postgres setup, backup and restore scripts, local dev restart scripts, detached restart targets, and smoke tests.

### Verified

- `cargo check --workspace`
- `cargo test --workspace`
- Local endpoint smoke test
- Docker Compose app/Postgres startup and endpoint smoke test
- Backup and restore rehearsal against Docker Postgres
