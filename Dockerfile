# syntax=docker/dockerfile:1
FROM rust:1.98-slim-bookworm AS rust
RUN apt-get update && apt-get install -y --no-install-recommends pkg-config libssl-dev ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
RUN --mount=type=cache,target=/usr/local/cargo/registry --mount=type=cache,target=/app/target cargo build --release --locked -p nested-chess-server && cp target/release/nested-chess-server /server

FROM rust AS wasm
RUN rustup target add wasm32-unknown-unknown
RUN --mount=type=cache,target=/usr/local/cargo/registry cargo install wasm-bindgen-cli --version 0.2.105 --locked
RUN --mount=type=cache,target=/usr/local/cargo/registry cargo build --release --locked -p nested-chess-wasm --target wasm32-unknown-unknown
RUN wasm-bindgen target/wasm32-unknown-unknown/release/nested_chess_wasm.wasm --target web --out-dir /pkg

FROM node:24-bookworm-slim AS web
WORKDIR /app
COPY package.json package-lock.json ./
RUN npm ci
COPY . .
COPY --from=wasm /pkg ./web/pkg
RUN node scripts/setup-bot.mjs && node scripts/package-source.mjs && npx vite build

FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=rust /server /usr/local/bin/nested-chess-server
COPY --from=web /app/dist ./dist
USER 65532:65532
ENV LISTEN_ADDR=0.0.0.0:4180 STATIC_DIR=/app/dist
EXPOSE 4180
HEALTHCHECK --interval=10s --timeout=3s --start-period=15s CMD curl --fail --silent http://127.0.0.1:4180/api/v1/ready || exit 1
ENTRYPOINT ["nested-chess-server"]
