# Corroded CMS Product Requirements

This is the implementation plan for a small Rust-native CMS using Leptos SSR, Axum, PostgreSQL, and SQLx. Leptos SSR is a good fit for CRUD-style apps with server-rendered HTML and hydration, while Axum can host backend routes and SQLx gives explicit async PostgreSQL access.

Project Goal

Build a production-ready blog CMS with:

Public site

* Blog index
* Post detail pages
* Tag pages
* RSS feed
* SEO metadata
* Markdown rendering
* Image/media serving

Admin CMS

* Login/logout
* Create/edit/archive posts
* Draft/published workflow
* Markdown editor
* Image uploads
* Tags
* Slug management
* Basic user management

Backend

* Leptos SSR frontend
* Axum server
* PostgreSQL
* SQLx
* Session auth
* File/object storage
* Migrations
* Tests
* Docker deployment

⸻

Recommended Architecture

Use this stack:

| Layer | Choice |
| --- | --- |
| UI | Leptos SSR with hydration |
| Server | Axum |
| DB | PostgreSQL |
| DB access | SQLx with SQL migrations |
| Auth | Opaque cookie sessions, server-side session table, Argon2id password hashing |
| Markdown | pulldown-cmark |
| Sanitization | ammonia |
| Images | Local uploads first, S3-compatible storage post-MVP |
| Styling | Plain CSS in the app crate |
| Deployment | Docker image for the app plus PostgreSQL via Docker Compose for local development |

SQLx is selected for the MVP because the CMS schema is small and explicit SQL is easy to audit.

Implementation Readiness Decisions

The following decisions are part of the MVP contract and should be treated as fixed unless the PRD is deliberately revised.

MVP scope

* Build a single-site blog CMS. Multi-tenant sites are out of scope.
* Support one or more admin users in the database, but only admin role behavior is required for MVP.
* Basic user management for MVP means creating the first admin from the CLI and allowing the authenticated admin to update their own display name and password. Full user CRUD, invites, editor/author permissions, and multi-author public pages are post-MVP.
* Comments, themes, public search, scheduled-publishing automation, revision restore, and S3-compatible object storage are post-MVP.

Workspace and runtime

* Convert the current package into a Rust workspace with `app/`, `server/`, `shared/`, and `migrations/`.
* The server binary owns configuration, database pooling, migrations, Axum routing, static asset serving, and Leptos SSR integration.
* The app crate owns Leptos routes, components, metadata, forms, and styles.
* The shared crate owns validation, slug generation, DTOs, Markdown rendering helpers, RSS/sitemap helpers, and testable pure logic.
* Use `cargo-leptos` for local SSR development and production builds.

Configuration

Required environment variables:

* `DATABASE_URL`
* `BASE_URL`
* `SESSION_SECRET`
* `UPLOAD_DIR`
* `ENVIRONMENT`
* `SITE_NAME`
* `SITE_DESCRIPTION`

Optional environment variables:

* `RUST_LOG`
* `HOST` defaulting to `127.0.0.1`
* `PORT` defaulting to `3000`
* `MAX_UPLOAD_BYTES` defaulting to `5242880`

`SESSION_SECRET` must be at least 32 bytes in production. `BASE_URL` is the canonical absolute site URL used for SEO, sitemap, RSS, and generated media URLs.

Database conventions

* Use PostgreSQL `uuid` primary keys generated with `gen_random_uuid()`.
* Store timestamps as `timestamptz`.
* Run SQLx SQL migrations from `migrations/`.
* Use database constraints for unique slugs, unique emails, valid statuses, valid roles, and foreign keys.
* Public post visibility is `status = 'published'`, `published_at IS NOT NULL`, and `published_at <= now()`.
* Slugs are globally unique per table: post slugs are unique across posts, tag slugs are unique across tags.
* Archived posts and archived tags remain in the database and are hidden from public pages.

Minimum schema fields

* `users`: `id`, `email`, `password_hash`, `display_name`, `role`, `created_at`, `updated_at`.
* `sessions`: `id`, `user_id`, `token_hash`, `expires_at`, `created_at`, `last_seen_at`.
* `posts`: `id`, `title`, `slug`, `excerpt`, `body_markdown`, `body_html`, `status`, `cover_image_id`, `author_id`, `published_at`, `scheduled_for`, `created_at`, `updated_at`.
* `tags`: `id`, `name`, `slug`, `archived_at`, `created_at`, `updated_at`.
* `post_tags`: `post_id`, `tag_id`, composite primary key on both fields.
* `media_assets`: `id`, `filename`, `original_filename`, `mime_type`, `size_bytes`, `storage_path`, `width`, `height`, `alt_text`, `uploaded_by`, `created_at`, `updated_at`.
* MVP `users.role` allowed value is `admin`; future roles require a migration and authorization changes.

Validation rules

* Email must be normalized to lowercase and trimmed before storage.
* Slugs are lowercase ASCII and must match `[a-z0-9]+(?:-[a-z0-9]+)*`.
* Title max length: 200 characters.
* Post slug max length: 200 characters.
* Excerpt max length: 500 characters.
* Markdown body max length: 1 MiB.
* Tag name max length: 80 characters.
* Tag slug max length: 100 characters.
* Display name max length: 100 characters.
* Media alt text max length: 255 characters.
* Passwords must be 12 to 128 characters.
* All validation must run server-side even when client-side validation exists.

Authentication and sessions

* Passwords use Argon2id with a per-password random salt.
* Session cookies use the name `corroded_session`.
* Store only a hash of the session token in the `sessions` table.
* Session cookies are `HttpOnly`, `SameSite=Lax`, `Path=/`, and `Secure` when `ENVIRONMENT=production`.
* Session lifetime is 14 days with expiration stored server-side.
* Logout deletes the server-side session and expires the cookie.
* State-changing admin actions must require CSRF validation.

Content rendering

* Convert Markdown to HTML on save and store both Markdown and sanitized HTML.
* Markdown preview must use the same server-side rendering and sanitization path as published posts.
* Allow fenced code blocks, tables, strikethrough, task lists when supported by the selected parser, links, images, headings, lists, and blockquotes.
* Sanitized HTML must remove scripts, event handlers, dangerous URLs, and unsafe embedded content.

Public routes

* `/` shows the latest 5 published posts.
* `/blog` lists published posts, newest first.
* `/blog/:slug` renders a single published post.
* `/tags/:slug` lists published posts for a tag.
* `/feed.xml` is the canonical RSS route.
* `/rss.xml` may redirect to `/feed.xml`.
* `/sitemap.xml` lists public canonical URLs.
* `/uploads/*path` serves uploaded media from `UPLOAD_DIR`.
* `/healthz` checks application and database connectivity.
* Missing public resources return 404, not redirects.

Admin routes

* `/admin/login` handles login.
* `/admin/logout` handles logout.
* `/admin` shows the dashboard.
* `/admin/posts` lists posts.
* `/admin/posts/new` creates a post.
* `/admin/posts/:id/edit` edits a post.
* `/admin/tags` manages tags.
* `/admin/media` manages uploads.
* `/admin/account` manages the current admin profile and password.
* All `/admin/*` routes except `/admin/login` require an authenticated session.

Pagination and ordering

* Public blog index page size defaults to 10 posts.
* Admin post list page size defaults to 25 posts.
* RSS includes the latest 20 published posts.
* Public lists sort by `published_at DESC, created_at DESC`.
* Admin lists sort by `updated_at DESC`.

Uploads

* Accept PNG, JPEG, WebP, and GIF images only.
* Enforce `MAX_UPLOAD_BYTES`; default max upload size is 5 MiB.
* Validate MIME type by file content, not only extension.
* Store files under `$UPLOAD_DIR/YYYY/MM/<uuid>.<ext>`.
* Public URLs use `/uploads/YYYY/MM/<uuid>.<ext>`.
* Preserve original filenames only as metadata.
* Never trust user-provided paths or filenames.

SEO, feeds, and metadata

* Use `SITE_NAME`, `SITE_DESCRIPTION`, and `BASE_URL` for site-level metadata.
* Use post title and excerpt for per-post title and description.
* Use cover image when present for Open Graph and Twitter card image.
* RSS item GUIDs use canonical post URLs.
* Sitemap includes homepage, blog index, tag pages with at least one published post, and published post pages.

Testing expectations

* Pure shared logic must have unit tests.
* Database-backed integration tests run against a test PostgreSQL database.
* The MVP is not complete until `cargo check` and the available test suite pass locally.

⸻

Phase 0 — Product Scope & Repo Setup

Milestone 0.1: Define MVP

CMS-0001 — Define content model

Tasks

* Define fields for posts:
    * id
    * title
    * slug
    * excerpt
    * body_markdown
    * body_html
    * status
    * cover_image_id
    * author_id
    * published_at
    * scheduled_for
    * created_at
    * updated_at
* Define statuses:
    * draft
    * published
    * archived
* Define tags:
    * id
    * name
    * slug
    * archived_at
    * created_at
    * updated_at

Acceptance criteria

* Finalized schema draft exists.
* MVP excludes comments, multi-author workflows, and themes.

⸻

CMS-0002 — Use SQLx

Decision
Use SQLx with SQL migration files for the MVP.

Acceptance criteria

* SQLx is configured for PostgreSQL.
* Migration approach is documented.

⸻

CMS-0003 — Initialize repo

Tasks

* Create Rust workspace:
    * app/
    * server/
    * shared/
    * migrations/
* Add Leptos SSR setup.
* Add Axum integration.
* Add .env.example.
* Add docker-compose.yml with PostgreSQL.
* Add justfile or Makefile.

Acceptance criteria

* cargo check passes.
* Local PostgreSQL runs.
* Empty Leptos page renders.

⸻

Phase 1 — Foundation

Milestone 1.1: App shell and routing

CMS-0101 — Create public layout

Tasks

* Add site header.
* Add footer.
* Add base HTML metadata.
* Add global CSS.
* Add responsive layout.

Acceptance criteria

* Homepage renders.
* Layout works on mobile and desktop.

⸻

CMS-0102 — Create admin layout

Tasks

* Add /admin route.
* Add admin sidebar.
* Add protected route placeholder.
* Add dashboard placeholder.

Acceptance criteria

* /admin renders only placeholder for now.
* Public and admin layouts are visually distinct.

⸻

Milestone 1.2: Database foundation

CMS-0110 — Add database migrations

Use SQLx SQL migration files. The app may use SQLx embedded migrations via `sqlx::migrate!`.

Tables

* users
* posts
* tags
* post_tags
* media_assets
* sessions

Acceptance criteria

* Fresh database migrates successfully.
* Schema is committed.

⸻

CMS-0111 — Add DB connection pool

Tasks

* Read DATABASE_URL.
* Create Postgres pool on server startup.
* Inject app state into Axum routes and Leptos server functions.

Acceptance criteria

* App starts only with valid DB connection.
* Health endpoint verifies DB connectivity.

⸻

Phase 2 — Authentication

Milestone 2.1: Admin user auth

CMS-0201 — Create user model

Fields

* id
* email
* password_hash
* display_name
* role
* created_at
* updated_at

Acceptance criteria

* User table exists.
* Unique email enforced.
* Role defaults to admin.

⸻

CMS-0202 — Seed first admin

Tasks

* Add CLI command:
    * corroded-cms create-admin --email email@example.com
* Prompt for password without echoing it by default.
* Hash password with Argon2id.
* Store admin user.

Acceptance criteria

* First admin can be created locally.
* Additional admin users can be created with the same CLI command.
* Duplicate admin email is rejected.
* Plain password is never stored.

⸻

CMS-0203 — Implement login

Tasks

* Add /admin/login.
* Validate email/password.
* Create secure cookie session.
* Redirect to /admin.

Acceptance criteria

* Valid login works.
* Invalid login shows generic error.
* Session cookie is HttpOnly, Secure in production, and SameSite=Lax.

⸻

CMS-0204 — Implement logout

Tasks

* Destroy session.
* Clear cookie.
* Redirect to login.

Acceptance criteria

* Logged-out user cannot access /admin.

⸻

CMS-0205 — Add auth guard

Tasks

* Protect all /admin/* routes except login.
* Add current-user server function.
* Add role check helper.

Acceptance criteria

* Anonymous users are redirected.
* Authenticated users can access dashboard.

⸻

CMS-0206 — Add account settings

Tasks

* Add authenticated account settings page.
* Allow current admin to update display name.
* Allow current admin to change password after confirming current password.
* Rehash changed password with Argon2id.

Acceptance criteria

* Admin can update their own display name.
* Admin can change their own password.
* Incorrect current password does not reveal whether a user exists.

⸻

Phase 3 — Posts MVP

Milestone 3.1: Post CRUD

CMS-0301 — Create post list in admin

Tasks

* Add /admin/posts.
* Display title, status, updated date, published date.
* Add filters:
    * all
    * draft
    * published
    * archived

Acceptance criteria

* Admin can view post list.
* Empty state appears when no posts exist.

⸻

CMS-0302 — Create post editor

Tasks

* Add /admin/posts/new.
* Fields:
    * title
    * slug
    * excerpt
    * body markdown
    * status
* Auto-generate slug from title.
* Allow manual slug override.

Acceptance criteria

* Admin can save a draft.
* Slug is unique.
* Validation errors are shown inline.

⸻

CMS-0303 — Edit existing post

Tasks

* Add /admin/posts/:id/edit.
* Load existing post.
* Save changes.
* Track updated_at.

Acceptance criteria

* Existing post can be edited.
* Slug collision is prevented.

⸻

CMS-0304 — Archive post

Tasks

* Add “Archive” action.
* Hide archived posts from public site.
* Keep archived posts visible in admin filter.

Acceptance criteria

* Archived post disappears from public blog.
* Admin can still find it.

⸻

Milestone 3.2: Markdown rendering

CMS-0310 — Convert Markdown to HTML

Tasks

* Convert Markdown on save.
* Sanitize generated HTML.
* Store both Markdown and HTML.

Acceptance criteria

* Markdown preview and public post display correctly.
* Unsafe scripts are removed.
* Code blocks render correctly.

⸻

CMS-0311 — Add Markdown preview

Tasks

* Add preview pane in editor.
* Add “Edit / Preview” toggle or split view.

Acceptance criteria

* Preview matches public rendering closely.

⸻

Phase 4 — Public Blog

Milestone 4.1: Public post rendering

CMS-0401 — Blog index

Route
/blog

Tasks

* List published posts.
* Sort by published_at DESC.
* Show title, excerpt, date, tags.

Acceptance criteria

* Only published posts appear.
* Drafts never appear publicly.

⸻

CMS-0402 — Post detail page

Route
/blog/:slug

Tasks

* Render title.
* Render date.
* Render body HTML.
* Render tags.
* Add canonical URL.

Acceptance criteria

* Published post loads by slug.
* Draft slug returns 404.
* Future-dated published_at returns 404 until the timestamp is reached.
* Missing slug returns 404.

⸻

CMS-0403 — Homepage integration

Tasks

* Show recent posts on /.
* Add link to blog index.

Acceptance criteria

* Homepage includes latest published posts.

⸻

Milestone 4.2: SEO

CMS-0410 — Add metadata

Tasks

* Per-post title.
* Description from excerpt.
* Open Graph tags.
* Twitter card tags.
* Canonical URL.

Acceptance criteria

* View-source contains correct metadata.
* Social preview has title, description, image when available.

⸻

CMS-0411 — Add sitemap

Route
/sitemap.xml

Tasks

* Include homepage.
* Include blog index.
* Include published posts.
* Exclude drafts and archived posts.

Acceptance criteria

* XML validates.
* Published posts appear.

⸻

Phase 5 — Tags

Milestone 5.1: Tag management

CMS-0501 — Add tag CRUD

Tasks

* Add /admin/tags.
* Create tag.
* Edit tag.
* Archive tag.
* Generate unique slug.

Acceptance criteria

* Admin can manage tags.
* Duplicate tag names/slugs prevented.
* Archived tags are hidden publicly but retained for existing admin post records.

⸻

CMS-0502 — Assign tags to posts

Tasks

* Add tag selector to post editor.
* Allow creating new tag inline.
* Store in post_tags.

Acceptance criteria

* Post can have multiple tags.
* Tags render on public post.

⸻

CMS-0503 — Public tag pages

Route
/tags/:slug

Tasks

* Show published posts for tag.
* Add tag metadata.

Acceptance criteria

* Tag page excludes drafts.
* Empty or missing tag returns 404.

⸻

Phase 6 — Drafts and Publishing Workflow

Milestone 6.1: Draft lifecycle

CMS-0601 — Implement publish action

Tasks

* Add “Publish” button.
* Set status = published.
* Set published_at if empty.
* Keep original published date on later edits unless explicitly changed.

Acceptance criteria

* Draft becomes public after publish.
* Published date is stable.

⸻

CMS-0602 — Implement unpublish action

Tasks

* Add “Unpublish” button.
* Set status = draft.
* Keep published_at for history.

Acceptance criteria

* Unpublished post disappears publicly.

⸻

CMS-0603 — Add scheduled publishing placeholder

MVP version
Add scheduled_for field but do not automate yet.

Acceptance criteria

* Schema supports future scheduling.
* UI can hide field behind “Advanced.”

⸻

Phase 7 — Image Uploads and Media Library

Milestone 7.1: Local media uploads

CMS-0701 — Create media table

Fields

* id
* filename
* original_filename
* mime_type
* size_bytes
* storage_path
* width
* height
* alt_text
* uploaded_by
* created_at
* updated_at

Acceptance criteria

* Media metadata is stored.

⸻

CMS-0702 — Upload endpoint

Tasks

* Accept image upload.
* Validate MIME type.
* Enforce max file size.
* Generate safe filename.
* Store under /uploads/YYYY/MM/.
* Insert DB record.

Acceptance criteria

* Admin can upload PNG/JPEG/WebP/GIF.
* Invalid files are rejected.
* Filename traversal is impossible.

⸻

CMS-0703 — Serve uploaded images

Tasks

* Add Axum static file serving for uploads.
* Add cache headers.
* Add public URL generation.

Acceptance criteria

* Uploaded image is accessible publicly.
* Cache headers are present.

⸻

CMS-0704 — Add cover image support

Tasks

* Add media picker in post editor.
* Set cover_image_id.
* Render cover image on blog index and post page.

Acceptance criteria

* Post can display cover image.
* Missing image does not break page.

⸻

CMS-0705 — Insert images into Markdown

Tasks

* Add “Insert image” action.
* Generate Markdown:
    * ![alt text](/uploads/...)

Acceptance criteria

* Image can be inserted into post body.
* Preview renders image.

⸻

CMS-0706 — Add media library page

Tasks

* Add /admin/media.
* List uploaded assets with thumbnail, original filename, dimensions, size, and upload date.
* Allow editing alt text.
* Allow copying or inserting public media URL.

Acceptance criteria

* Admin can inspect uploaded media.
* Admin can update alt text.
* Broken or missing files are handled without breaking the page.

⸻

Phase 8 — RSS Feed

Milestone 8.1: RSS

CMS-0801 — Add RSS route

Route
/feed.xml

Tasks

* Generate RSS 2.0 feed.
* Include latest published posts.
* Include title, link, description, pubDate, guid.
* Use excerpt or sanitized summary.
* Redirect /rss.xml to /feed.xml.

Acceptance criteria

* Feed validates.
* Drafts are excluded.
* Feed updates when posts publish.

⸻

CMS-0802 — Add feed discovery

Tasks

* Add <link rel="alternate" type="application/rss+xml"> to layout.

Acceptance criteria

* Browsers/feed readers can discover feed.

⸻

Phase 9 — Admin Polish

Milestone 9.1: Usability

CMS-0901 — Dashboard

Tasks

* Show counts:
    * published posts
    * drafts
    * tags
    * media assets
* Show recent edited posts.

Acceptance criteria

* Dashboard gives useful summary.

⸻

CMS-0902 — Search posts

Tasks

* Add search input to admin post list.
* Search title, slug, excerpt.

Acceptance criteria

* Admin can find posts quickly.

⸻

CMS-0903 — Autosave drafts

Tasks

* Debounced save every 10–30 seconds.
* Show save state:
    * saved
    * saving
    * failed

Acceptance criteria

* Draft content is not easily lost.
* Failed autosave is visible.

⸻

CMS-0904 — Revision history

Optional but valuable

Tasks

* Add post_revisions.
* Save snapshot on publish or manual save.
* Allow viewing previous revisions.

Acceptance criteria

* Admin can inspect prior versions.
* Restore can be deferred.

⸻

Phase 10 — Security Hardening

Milestone 10.1: App security

CMS-1001 — Input validation

Tasks

* Validate all forms server-side.
* Enforce max lengths.
* Normalize slugs.
* Reject invalid statuses.

Acceptance criteria

* Invalid data cannot be inserted via direct requests.

⸻

CMS-1002 — CSRF protection

Tasks

* Add CSRF token for state-changing admin actions.
* Validate token server-side.

Acceptance criteria

* POST/PUT/DELETE without token fails.

⸻

CMS-1003 — Rate limit login

Tasks

* Limit failed login attempts by IP/email.
* Add generic error messages.

Acceptance criteria

* Brute-force attempts are slowed.

⸻

CMS-1004 — Harden uploads

Tasks

* Verify image MIME by content, not only extension.
* Strip dangerous filenames.
* Cap upload size.
* Defer image re-encoding unless validation proves insufficient.

Acceptance criteria

* Non-image uploads are rejected.
* Oversized images are rejected.

⸻

CMS-1005 — Security headers

Tasks

* Add:
    * Content-Security-Policy
    * X-Content-Type-Options
    * Referrer-Policy
    * Permissions-Policy

Acceptance criteria

* Headers appear on public and admin pages.

⸻

Phase 11 — Testing

Milestone 11.1: Automated tests

CMS-1101 — Unit tests

Cover

* Slug generation
* Markdown sanitization
* Validation rules
* RSS generation
* Sitemap generation

Acceptance criteria

* Unit tests pass in CI.

⸻

CMS-1102 — Integration tests

Cover

* Create post
* Publish post
* Draft is hidden publicly
* Login/logout
* Upload image
* RSS includes published post

Acceptance criteria

* Tests run against test PostgreSQL database.

⸻

CMS-1103 — Browser tests

Optional
Use Playwright or similar.

Cover

* Admin login
* Create post
* Publish post
* View public post

Acceptance criteria

* One happy-path E2E test passes.

⸻

Phase 12 — Deployment

Milestone 12.1: Production packaging

CMS-1201 — Dockerize app

Tasks

* Multi-stage Dockerfile.
* Runtime image includes binary, static assets, migrations.
* Environment-based config.

Acceptance criteria

* App runs from Docker image.
* Static assets load.

⸻

CMS-1202 — Add production config

Environment variables

* DATABASE_URL
* BASE_URL
* SESSION_SECRET
* UPLOAD_DIR
* ENVIRONMENT
* SITE_NAME
* SITE_DESCRIPTION

Optional environment variables

* RUST_LOG
* HOST
* PORT
* MAX_UPLOAD_BYTES

Acceptance criteria

* App fails fast if required config is missing.

⸻

CMS-1203 — Migration on deploy

Tasks

* Run migrations on startup or as separate release command.
* Prefer separate command for production.

Acceptance criteria

* New deployment can upgrade DB safely.

⸻

CMS-1204 — Reverse proxy setup

Tasks

* Configure Caddy, Nginx, or Traefik.
* Enable HTTPS.
* Proxy to app port.
* Serve uploads either through app or proxy.

Acceptance criteria

* Public site works over HTTPS.
* Admin cookies are secure.

⸻

Phase 13 — Observability and Maintenance

Milestone 13.1: Logging and monitoring

CMS-1301 — Structured logs

Tasks

* Add tracing.
* Log request IDs.
* Log errors without secrets.

Acceptance criteria

* Production logs are readable.

⸻

CMS-1302 — Error pages

Tasks

* Custom 404.
* Custom 500.
* Admin error boundary.

Acceptance criteria

* User-friendly error pages exist.

⸻

CMS-1303 — Backups

Tasks

* PostgreSQL backup script.
* Uploads backup script.
* Restore instructions.

Acceptance criteria

* Backup and restore tested once.

⸻

Phase 14 — Post-MVP Features

Suggested backlog

CMS-1401 — Full-text search

* Use PostgreSQL full-text search.
* Public search page.
* Admin search improvements.

CMS-1402 — Scheduled publishing

* Background job checks scheduled_for.
* Publish posts automatically.

CMS-1403 — Multiple authors

* Author profile pages.
* Author archive pages.

CMS-1404 — Roles and permissions

* Admin
* Editor
* Author

CMS-1405 — Object storage

* S3-compatible storage.
* Cloudflare R2, MinIO, Backblaze B2, or AWS S3.

CMS-1406 — Theme system

* Plugin architecture: each theme owns its shell, public templates, assets, default navigation, and footer groups behind a stable server-side theme plugin boundary.
* Admin theme manager for reviewing registered plugins and switching the active theme.
* Configurable active theme ID with validation against the registered plugin list and persistence in the site settings table.
* Site settings table for editable site-level content and custom navigation.
* GigaTier remains the first built-in/default theme plugin and reference implementation.

CMS-1407 — Comments

* Moderation queue.
* Spam prevention.
* Optional anonymous comments.

CMS-1408 — CMS-managed pages and navigation

* First-class `pages` content type with title, slug/path, body Markdown/HTML, status, template, and SEO metadata.
* Admin CRUD for pages at `/admin/pages`.
* Public page routing with reserved-path protection for `/admin`, `/blog`, `/tags`, feeds, uploads, and other system routes.
* Navigation manager for primary/footer links with ordering, labels, internal page targets, and custom URLs.
* Theme plugins should render stored navigation and use plugin defaults only as fallback.

Post-MVP implementation detail: pages, navigation, and editable themes

The next coherent step after the MVP and initial theme plugin work is not more hard-coded theme content. It is CMS-managed site structure:

1. Add first-class pages.
2. Add navigation management.
3. Make theme plugins render stored pages/navigation/settings.
4. Add editable theme instances using a Rust runtime template engine.

This keeps the system aligned with a real CMS: authors edit content and navigation in the admin UI, while theme developers can still ship robust Rust-backed default themes.

CMS-managed pages

Pages are distinct from posts:

* Posts are chronological content and live under `/blog/{slug}`.
* Pages are durable site content and should be routable at stable paths such as `/about`, `/contact`, `/product`, or nested paths such as `/company/security`.

Suggested schema:

```sql
CREATE TABLE pages (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    title text NOT NULL CHECK (char_length(title) <= 200),
    slug text NOT NULL CHECK (char_length(slug) <= 200),
    path text NOT NULL UNIQUE CHECK (char_length(path) <= 300),
    excerpt text NOT NULL DEFAULT '' CHECK (char_length(excerpt) <= 500),
    body_markdown text NOT NULL DEFAULT '' CHECK (octet_length(body_markdown) <= 1048576),
    body_html text NOT NULL DEFAULT '',
    status text NOT NULL DEFAULT 'draft' CHECK (status IN ('draft', 'published', 'archived')),
    template_key text NOT NULL DEFAULT 'page' CHECK (char_length(template_key) <= 100),
    meta_title text CHECK (meta_title IS NULL OR char_length(meta_title) <= 200),
    meta_description text CHECK (meta_description IS NULL OR char_length(meta_description) <= 300),
    canonical_url text CHECK (canonical_url IS NULL OR char_length(canonical_url) <= 500),
    published_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);
```

Recommended indexes:

```sql
CREATE UNIQUE INDEX pages_public_path_idx ON pages (path)
WHERE status = 'published' AND published_at IS NOT NULL;

CREATE INDEX pages_updated_idx ON pages (updated_at DESC);
```

Admin requirements:

* `/admin/pages` lists pages with status, path, template, publish date, and updated date.
* `/admin/pages/new` creates a draft page.
* `/admin/pages/{id}/edit` edits page content and metadata.
* Page editor should support the existing Markdown workflow with preview.
* If rich text is added later, store canonical Markdown or sanitized HTML, not arbitrary unsanitized browser HTML.
* Page paths must be normalized to start with `/`, reject duplicate slashes, reject `..`, and reject trailing slash except `/`.
* Page paths must be checked against reserved system paths.

Reserved paths:

* `/admin`
* `/admin/*`
* `/blog`
* `/blog/*`
* `/tags`
* `/tags/*`
* `/uploads`
* `/uploads/*`
* `/feed.xml`
* `/rss.xml`
* `/sitemap.xml`
* `/healthz`
* `/pkg`
* `/pkg/*`
* `/themes`
* `/themes/*`

Public routing order:

1. Match explicit system routes first.
2. Match blog, tag, feed, sitemap, admin, uploads, and static assets.
3. Fall back to published page lookup by normalized request path.
4. Return 404 only if no system route or page route matches.

Navigation management

Navigation should be CMS data, not hard-coded theme arrays.

Suggested schema:

```sql
CREATE TABLE navigation_menus (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    key text NOT NULL UNIQUE CHECK (char_length(key) <= 80),
    label text NOT NULL CHECK (char_length(label) <= 120),
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE navigation_items (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    menu_id uuid NOT NULL REFERENCES navigation_menus(id) ON DELETE CASCADE,
    parent_id uuid REFERENCES navigation_items(id) ON DELETE CASCADE,
    label text NOT NULL CHECK (char_length(label) <= 120),
    url text CHECK (url IS NULL OR char_length(url) <= 500),
    page_id uuid REFERENCES pages(id) ON DELETE SET NULL,
    position integer NOT NULL DEFAULT 0,
    open_in_new_tab boolean NOT NULL DEFAULT false,
    archived_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CHECK (
        (page_id IS NOT NULL AND url IS NULL)
        OR (page_id IS NULL AND url IS NOT NULL)
    )
);
```

Admin requirements:

* `/admin/navigation` lists menus.
* Each menu supports ordered items.
* Items can link to an internal page or a custom URL.
* Internal page links should use the current page path at render time so path edits do not strand old navigation rows.
* Archived pages should be clearly flagged in navigation editing.
* The primary menu and footer menu should be seeded from the active theme defaults on first install.
* Theme defaults are fallback only; once a menu exists in the database, stored menu data wins.

Rendering requirements:

* Public render context must include `navigation.primary` and `navigation.footer`.
* Theme plugins should render stored navigation through common helpers rather than static arrays.
* If stored navigation is empty, the active theme may expose fallback nav items.

Hybrid theme architecture

Corroded CMS should use a hybrid theme model:

1. Rust theme plugins for trusted built-in themes and package metadata.
2. Runtime template themes for admin-editable theme markup, CSS, and settings.

Rust plugins are compiled. They are suitable for shipping defaults, schemas, helper registrations, migrations, and packaged assets. They are not suitable for in-browser template CRUD because changing Rust code requires rebuilding the binary.

Editable themes should be represented as data:

* theme records
* template records
* setting records
* asset records
* generated CSS or editable CSS
* page and navigation data rendered through the active theme

Use an existing Rust runtime template engine rather than inventing a template language.

Preferred engine:

* MiniJinja.

Why MiniJinja is a good fit:

* It is Rust-native.
* It supports runtime-loaded templates from strings or files.
* It accepts `serde` context values.
* It supports inheritance, includes, blocks, filters, and functions.
* It has safety controls such as fuel that can help prevent expensive templates.
* Its syntax is familiar enough for theme authors and has existing editor support.

Acceptable alternatives:

* Tera for a Jinja/Django-style Rust runtime engine.
* Liquid if a more constrained designer-facing language is preferred.
* Handlebars if simple substitution and helpers are enough.

Do not use Askama for editable themes. Askama is compile-time and type-safe, which is useful for built-in server/admin templates, but it does not satisfy browser-editable theme requirements.

Theme records

Suggested schema:

```sql
CREATE TABLE themes (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    key text NOT NULL UNIQUE CHECK (char_length(key) <= 100),
    display_name text NOT NULL CHECK (char_length(display_name) <= 160),
    source text NOT NULL CHECK (source IN ('builtin', 'editable', 'package')),
    version text CHECK (version IS NULL OR char_length(version) <= 80),
    description text NOT NULL DEFAULT '' CHECK (char_length(description) <= 1000),
    preview_image_path text CHECK (preview_image_path IS NULL OR char_length(preview_image_path) <= 500),
    archived_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE theme_templates (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    theme_id uuid NOT NULL REFERENCES themes(id) ON DELETE CASCADE,
    key text NOT NULL CHECK (char_length(key) <= 120),
    source text NOT NULL CHECK (octet_length(source) <= 1048576),
    content_type text NOT NULL DEFAULT 'text/html' CHECK (char_length(content_type) <= 120),
    updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (theme_id, key)
);

CREATE TABLE theme_settings (
    theme_id uuid NOT NULL REFERENCES themes(id) ON DELETE CASCADE,
    key text NOT NULL CHECK (char_length(key) <= 120),
    value_json jsonb NOT NULL DEFAULT 'null'::jsonb,
    updated_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (theme_id, key)
);

CREATE TABLE theme_assets (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    theme_id uuid NOT NULL REFERENCES themes(id) ON DELETE CASCADE,
    path text NOT NULL CHECK (char_length(path) <= 500),
    media_asset_id uuid REFERENCES media_assets(id) ON DELETE SET NULL,
    content_type text NOT NULL CHECK (char_length(content_type) <= 120),
    archived_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (theme_id, path)
);
```

Active theme setting:

* Continue storing active theme selection in `site_settings` as `theme.active`.
* `theme.active` should reference `themes.key` or a registered built-in plugin key.
* The active theme cannot be archived.
* If the active theme is missing or archived, the server must fall back to the configured default and log an error.

Theme template keys

A theme should provide these template keys at minimum:

* `base.html`
* `home.html`
* `page.html`
* `blog_index.html`
* `post_detail.html`
* `tag_index.html`
* `error.html`
* `partials/nav.html`
* `partials/footer.html`

Template validation:

* Saving a template must parse/compile it before persistence.
* The admin UI must show template errors with line and column if available.
* Activation must validate that all required template keys exist.
* Missing optional templates should fall back to theme defaults or `base.html`.
* Template includes must be restricted to the active theme namespace and approved partials.
* Template source size must be capped.
* Rendering must use autoescaping for HTML templates.

Render context contract

All runtime themes should receive a stable serializable context. Example shape:

```json
{
  "site": {
    "name": "GigaTier Technologies",
    "description": "Autonomous C/C++ to Rust transpilation.",
    "base_url": "http://127.0.0.1:3000"
  },
  "request": {
    "path": "/about"
  },
  "theme": {
    "key": "gigatier",
    "settings": {}
  },
  "navigation": {
    "primary": [],
    "footer": []
  },
  "page": {},
  "post": {},
  "posts": [],
  "tags": []
}
```

Rules:

* Do not expose secrets, session data, CSRF tokens, database URLs, upload directory paths, or admin-only fields to public templates.
* Public templates should receive only sanitized public content.
* Markdown-generated HTML should be sanitized before it enters the render context.
* User-authored raw HTML, if supported, must be sanitized with an allowlist.
* Theme helper functions must be explicitly registered. Do not expose arbitrary file, network, process, or environment access.

Theme CRUD semantics

Create:

* Built-in themes are created by registering Rust plugins and seeding a `themes` record.
* Editable themes can be created from a blank starter, from a built-in theme copy, or from an imported package.
* Creating from a built-in theme should copy editable templates/settings into database records while preserving the original built-in package.
* New themes start inactive until validation passes.

Read:

* `/admin/themes` lists installed themes, active status, source type, version, archive status, and validation status.
* Detail view shows templates, settings, assets, navigation defaults, preview image, and required template coverage.
* Admins can preview a theme against existing pages/posts without activating it.

Update:

* Editable themes can update templates, settings, CSS, and assets.
* Built-in themes can expose editable settings, but built-in template source should remain read-only unless copied into an editable child theme.
* Template saves must invalidate the compiled template cache for that theme.
* CSS edits should be saved as theme assets or a dedicated `theme_templates` row with `content_type = 'text/css'`.
* Theme settings should be generated from a plugin-provided schema where possible.

Delete/archive:

* Use `archived_at`; do not hard-delete by default.
* Active theme cannot be archived.
* Archived themes cannot be activated.
* Existing pages and navigation must keep rendering with the active fallback if their prior theme is archived.
* Permanent deletion can be a later admin-only maintenance operation after reference checks.

Theme package format

A package import format should be added before supporting arbitrary uploads. Suggested layout:

```text
theme.toml
templates/
  base.html
  home.html
  page.html
  blog_index.html
  post_detail.html
  tag_index.html
  error.html
  partials/nav.html
  partials/footer.html
assets/
  style.css
  logo.svg
  preview.png
settings.schema.json
```

`theme.toml` should include:

```toml
key = "gigatier"
display_name = "GigaTier"
version = "1.0.0"
engine = "minijinja"
description = "GigaTier public site theme."
```

Package import requirements:

* Reject path traversal and absolute paths.
* Reject executable files.
* Cap total package size and per-file size.
* Validate manifest before importing assets/templates.
* Parse all templates before marking import successful.
* Store imported package files as database rows or copy them into a controlled theme asset directory.

Admin editor requirements

Pages:

* Markdown editor with preview, using the same sanitation path as posts.
* SEO fields.
* Template selector limited to templates supported by the active theme.
* Publish/draft/archive workflow.

Navigation:

* Menu selector for primary/footer.
* Ordered link list.
* Link target can be internal page or custom URL.
* Validation prevents empty labels, invalid URLs, and archived page targets unless explicitly allowed.

Themes:

* Installed themes list.
* Active theme selector.
* Editable theme create/copy/archive actions.
* Template editor for runtime templates.
* Theme settings form generated from schema.
* Asset manager scoped to the theme.
* Preview action that renders a selected page/post using the candidate theme without changing `theme.active`.

The template editor is not a WYSIWYG page editor. It is a code editor for theme authors. WYSIWYG or Markdown editing belongs to pages/posts and structured theme settings.

Runtime rendering flow

1. Resolve active theme from `site_settings`.
2. Load theme plugin metadata or editable theme records.
3. Resolve navigation from database, falling back to theme defaults only if no stored menu exists.
4. Resolve content route.
5. Build the render context.
6. Select the template key.
7. Render with the runtime engine or built-in Rust plugin.
8. Apply global response headers and CSP.

Caching:

* Cache parsed runtime templates per theme version/update timestamp.
* Invalidate cache after template/settings/asset updates.
* Cache navigation and site settings with cheap invalidation after admin writes.
* Do not cache admin previews globally.

Security requirements:

* Runtime templates must use HTML autoescaping.
* Template engine functions must be allowlisted.
* Template recursion, loop cost, or render fuel must be bounded.
* Templates must not read arbitrary files.
* Templates must not make network requests.
* Templates must not access process environment.
* User content HTML must be sanitized before rendering.
* Theme assets must use safe content types.
* Public route fallback must not shadow admin/system routes.
* Admin theme CRUD requires an authenticated admin and CSRF protection.

Acceptance criteria

Pages/navigation:

* Admin can create a draft page.
* Draft page is not publicly visible.
* Admin can publish a page at `/about`.
* Published page renders at `/about`.
* Reserved path such as `/admin/test` is rejected.
* Admin can add page to primary navigation.
* Primary navigation renders stored database nav rather than theme defaults.
* Archiving a page removes or clearly flags it in navigation.

Editable themes:

* Admin can view installed themes.
* Admin can activate a non-archived theme.
* Admin cannot archive the active theme.
* Admin can create an editable copy of a built-in theme.
* Admin can edit an editable theme template and preview it.
* Invalid template changes are rejected with a useful error.
* Valid template changes render on public pages after activation.
* Smoke tests cover `/admin/themes`, `/admin/pages`, `/admin/navigation`, public page rendering, and reserved path rejection.

⸻

Suggested Tracking Board

Use these columns:

1. Backlog
2. Ready
3. In Progress
4. In Review
5. Testing
6. Done
7. Blocked

Use labels:

* phase-0-setup
* phase-1-foundation
* phase-2-auth
* phase-3-posts
* phase-4-public
* phase-5-tags
* phase-6-drafts
* phase-7-media
* phase-8-rss
* phase-9-admin
* phase-10-security
* phase-11-tests
* phase-12-deploy
* priority-critical
* priority-high
* priority-medium
* optional

⸻

MVP Cut Line

The first usable version should include only:

* Auth
* First admin creation and account settings
* Admin post CRUD
* Draft/published status
* Markdown rendering
* Public blog index
* Public post pages
* Tags
* Local image uploads
* RSS
* Docker deployment

Defer:

* Revisions
* Scheduled publishing
* Comments
* Multi-author roles
* S3 uploads
* Theme system
* Full-text search

⸻

Build Order Summary

1. Repo + Leptos SSR + Axum shell
2. PostgreSQL migrations
3. Auth
4. Admin post CRUD
5. Markdown rendering
6. Public blog pages
7. Tags
8. Draft/publish workflow
9. Image uploads
10. RSS + sitemap
11. Security hardening
12. Tests
13. Docker deployment

This gives you a clean path from empty repo to a usable Rust-native CMS without drifting into a giant WordPress clone.

⸻

Implementation Status

As of May 5, 2026, the MVP cut line is implemented and verified.

Completed MVP scope

* Auth.
* First admin creation and account settings.
* Admin post CRUD.
* Draft/published/archived status handling.
* Markdown rendering and server-rendered editor preview.
* Public blog index and public post detail pages.
* Tags, tag archives, and admin tag management.
* Local image uploads, cover images, alt text editing, and media snippets.
* RSS feed and sitemap.
* Security hardening: CSRF protection, secure response headers, request IDs, custom error pages, login rate limiting, and upload validation.
* Tests and smoke verification.
* Docker deployment.
* Backup and restore scripts.

Release verification

* `cargo check --workspace` passed.
* `cargo test --workspace` passed.
* Local endpoint smoke test passed.
* Docker Compose app/Postgres startup passed.
* Admin creation inside the Docker app container passed.
* Docker endpoint smoke test passed.
* Backup and restore rehearsal against Docker Postgres passed.

Deferred post-MVP scope remains unchanged: revisions, scheduled publishing automation, comments, multi-author roles, S3-compatible object storage, theme system, and full-text search.
