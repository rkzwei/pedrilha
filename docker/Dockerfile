# Stage 1: Build the API server
FROM rust:1.96-slim-bookworm AS builder

RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy manifests for dependency caching
COPY Cargo.toml Cargo.lock ./
COPY crates/shared/Cargo.toml crates/shared/
COPY crates/db/Cargo.toml crates/db/
COPY crates/api/Cargo.toml crates/api/

# Build dependencies (cached layer)
RUN mkdir -p crates/shared/src crates/db/src crates/api/src && \
    echo "fn main() {}" > crates/api/src/main.rs && \
    echo "" > crates/shared/src/lib.rs && \
    echo "" > crates/db/src/lib.rs && \
    cargo build --package gem-finder-api --release 2>/dev/null || true

# Copy actual source
COPY crates/shared/src crates/shared/src/
COPY crates/db/src crates/db/src/
COPY crates/api/src crates/api/src/

# Build the actual binary
RUN cargo build --package gem-finder-api --release

# Stage 2: Runtime
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy the binary
COPY --from=builder /app/target/release/gem-finder-api /app/gem-finder-api

# Expose the API port
EXPOSE 3000

# Run the API server
CMD ["/app/gem-finder-api"]