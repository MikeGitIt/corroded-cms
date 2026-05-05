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
