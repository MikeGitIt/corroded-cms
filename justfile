set dotenv-load := true

check:
    cargo check --workspace

test:
    cargo test --workspace

smoke:
    ./scripts/smoke.sh

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
