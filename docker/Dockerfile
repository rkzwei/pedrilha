# ─── Stage 1: Frontend (Tailwind CSS + Leptos WASM) ──────────────────────────
FROM rust:1.78-slim-bookworm AS frontend-builder

RUN apt-get update && apt-get install -y \
    curl ca-certificates pkg-config libssl-dev \
    && curl -fsSL https://deb.nodesource.com/setup_22.x | bash - \
    && apt-get install -y nodejs \
    && rm -rf /var/lib/apt/lists/*

# wasm32 compile target
RUN rustup target add wasm32-unknown-unknown

# trunk: download pre-built binary (much faster than cargo install)
RUN curl -fsSL \
    https://github.com/trunk-rs/trunk/releases/download/v0.21.14/trunk-x86_64-unknown-linux-gnu.tar.gz \
    | tar -xzC /usr/local/bin

# wasm-bindgen-cli: required by trunk
RUN cargo install wasm-bindgen-cli

WORKDIR /app
COPY . .

# Build Tailwind CSS then WASM frontend
RUN cd crates/frontend && npm install && npm run build:css
RUN trunk build --release

# ─── Stage 2: API binary ─────────────────────────────────────────────────────
FROM rust:1.78-slim-bookworm AS api-builder

RUN apt-get update && apt-get install -y pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Cache Cargo dependencies in a separate layer
COPY Cargo.toml Cargo.lock ./
COPY crates/shared/Cargo.toml  crates/shared/
COPY crates/db/Cargo.toml      crates/db/
COPY crates/api/Cargo.toml     crates/api/
COPY crates/frontend/Cargo.toml crates/frontend/

RUN mkdir -p crates/shared/src crates/db/src crates/api/src crates/frontend/src \
    && echo "fn main() {}" > crates/api/src/main.rs \
    && echo "fn main() {}" > crates/frontend/src/main.rs \
    && echo "" > crates/shared/src/lib.rs \
    && echo "" > crates/db/src/lib.rs \
    && SKIP_TRUNK_BUILD=1 cargo build --package gem-finder-api --release 2>/dev/null || true

# Copy real source
COPY crates/shared/src crates/shared/src/
COPY crates/db/src     crates/db/src/
COPY crates/api/src    crates/api/src/

# SKIP_TRUNK_BUILD: frontend is already built in stage 1
RUN SKIP_TRUNK_BUILD=1 cargo build --package gem-finder-api --release

# ─── Stage 3: Runtime image ───────────────────────────────────────────────────
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y ca-certificates libssl3 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=api-builder   /app/target/release/gem-finder-api ./gem-finder-api
COPY --from=frontend-builder /app/dist ./dist

ENV SERVE_FRONTEND=1
EXPOSE 3000
CMD ["./gem-finder-api"]
