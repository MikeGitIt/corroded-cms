set dotenv-load := true

check:
    cargo check --workspace

test:
    cargo test --workspace

smoke:
    ./scripts/smoke.sh

test-db-local:
    ./scripts/dev-local-db-test.sh

migrate:
    cargo run -p corroded-cms -- migrate

migrate-local:
    cargo build -p corroded-cms
    ./scripts/dev-local-migrate.sh

db-up:
    docker compose up -d postgres

dev:
    cargo leptos watch

restart:
    cargo leptos build
    ./scripts/dev-restart.sh

restart-local:
    cargo leptos build
    ./scripts/dev-local-restart.sh
