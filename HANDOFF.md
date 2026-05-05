# Corroded CMS Handoff

Date: 2026-05-05

## Current State

- Repo: `/Users/mickillah/Code/rust_projects/corroded-cms`
- Branch: `main`
- Working tree: contains in-progress theme manager changes plus this `HANDOFF.md`; `HANDOFF.md` was originally untracked, but the user has now asked to add new post-MVP notes here.
- Latest pushed commit: `f99234f Add GigaTier theme plugin`
- Release tag: `v0.1.0` points at `HEAD` and has been pushed.
- Remote name is `main`, not `origin`.

## User Constraints

- Do not run `cargo fmt` without explicit user permission.
- Keep updates concise. The user asked not to receive unnecessary play-by-play.
- Prefer scripted/repeatable checks over one-off terminal commands.
- Docker.app may need to be running for Docker verification.
- Local PostgreSQL client commands are versioned: `psql-17`, `pg_dump-17`, `pg_restore-17`.

## What Is Done

- MVP implementation is complete and tagged as `v0.1.0`.
- `SPECS/corroded-cms-prd.md` has an implementation status section.
- `CHANGELOG.md` exists for `0.1.0`.
- Docker runtime was verified by the user:
  - image built
  - Compose app/Postgres started
  - admin created inside app container
  - smoke test passed
  - Compose stack shut down
- Backup/restore rehearsal is scripted:
  - `scripts/backup-restore-rehearsal.sh`
  - `just rehearse-backup-restore`
  - It starts a disposable PostgreSQL 17 container, seeds a marker row and upload fixture, runs the existing backup/restore scripts, verifies restored DB/uploads, and removes the container via cleanup trap.
- Detached dev restart is scripted:
  - `just restart-bg`
  - `just restart-local-bg`
  - pid/log defaults: `/private/tmp/corroded-cms-3000.pid`, `/private/tmp/corroded-cms-3000.log`

## Verification Already Run

- `cargo check --workspace`
- `cargo test --workspace`
- `./scripts/smoke.sh`
- `just migrate-local`
- `just test-db-local`
- `just rehearse-backup-restore`
- Docker Compose app smoke test was run by the user and passed.

## Immediate Follow-Up

The last user concern was that the backup/restore rehearsal script should confirm cleanup itself rather than requiring a manual `docker ps` check afterward.

Current script behavior:

- `scripts/backup-restore-rehearsal.sh` removes the disposable container in `cleanup()`.
- It does not currently assert and print that the container is gone after `docker rm -f`.

Recommended patch:

- Update `cleanup()` in `scripts/backup-restore-rehearsal.sh` so it verifies `docker ps -a --format '{{.Names}}'` no longer contains `$CONTAINER_NAME`.
- Print a concise success line such as `Removed rehearsal container: corroded-cms-backup-rehearsal`.
- If the container still exists, print `FAIL: rehearsal container was not removed: ...` and exit nonzero.
- Run:

```bash
bash -n scripts/backup-restore-rehearsal.sh
just rehearse-backup-restore
```

- Commit/push as a follow-up patch. Do not move or recreate `v0.1.0` unless the user explicitly asks.

## Useful Commands

Local dev:

```bash
just restart-local-bg
BASE_URL='http://127.0.0.1:3000' ./scripts/smoke.sh
```

Docker verification:

```bash
docker compose --profile app up --build -d
docker compose --profile app exec -T app corroded-cms create-admin --email admin@corroded.local --display-name Admin --password 'TemporaryPass123!'
BASE_URL='http://127.0.0.1:3000' ./scripts/smoke.sh
docker compose --profile app down
```

Release tag status:

```bash
git log -1 --oneline
git tag --points-at HEAD
```

## Post-MVP Direction

Recommended first post-MVP track remains making CMS-managed site structure real. Current theme work provides a plugin boundary and admin theme manager, but there is still no CMS-managed page content type or navigation manager.

Add first-class pages/navigation:

- `pages` table with title, slug/path, Markdown/HTML body, status, template, and SEO fields.
- Admin CRUD at `/admin/pages`.
- Public page routing with reserved-path protection for `/admin`, `/blog`, `/tags`, feeds, uploads, and other system routes.
- Navigation manager for primary/footer nav items, order, labels, and internal page targets or custom URLs.
- Theme plugins should render stored navigation, falling back to plugin defaults when no nav has been configured.

Other deferred PRD items include S3-compatible media storage, full user management/invites, scheduled publishing automation, revision restore, admin/public search, and comments.
