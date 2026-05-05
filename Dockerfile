# syntax=docker/dockerfile:1

FROM rustlang/rust:nightly-bookworm AS builder

WORKDIR /app

RUN rustup target add wasm32-unknown-unknown \
    && cargo install cargo-leptos --locked

COPY Cargo.toml Cargo.lock ./
COPY app ./app
COPY server ./server
COPY shared ./shared
COPY migrations ./migrations

RUN cargo leptos build --release

FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=builder /app/target/release/corroded-cms /usr/local/bin/corroded-cms
COPY --from=builder /app/target/site ./target/site
COPY --from=builder /app/Cargo.toml ./Cargo.toml
COPY --from=builder /app/migrations ./migrations

RUN useradd --system --create-home --uid 10001 corroded \
    && mkdir -p /app/uploads \
    && chown -R corroded:corroded /app

USER corroded

ENV HOST=0.0.0.0 \
    PORT=3000 \
    UPLOAD_DIR=/app/uploads

EXPOSE 3000

CMD ["corroded-cms", "serve"]
