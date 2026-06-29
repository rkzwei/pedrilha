FROM ubuntu:22.04

ARG RUNNER_VERSION=2.319.1
ARG WASM_BINDGEN_VERSION=0.2.125
ARG TRUNK_VERSION=0.21.14
ARG NODE_MAJOR=22

ENV DEBIAN_FRONTEND=noninteractive
ENV CARGO_HOME=/home/runner/.cargo
ENV CARGO_TARGET_DIR=/home/runner/.cargo-target
ENV PATH="/home/runner/.cargo/bin:${PATH}"

# System deps
RUN apt-get update && apt-get install -y \
    curl git rsync openssh-client jq \
    build-essential pkg-config libssl-dev \
    ca-certificates gnupg \
    && rm -rf /var/lib/apt/lists/*

# Node.js
RUN curl -fsSL https://deb.nodesource.com/setup_${NODE_MAJOR}.x | bash - \
    && apt-get install -y nodejs \
    && rm -rf /var/lib/apt/lists/*

# Runner user
RUN useradd -m -s /bin/bash runner

# GitHub Actions runner binary
RUN mkdir -p /home/runner/actions-runner \
    && cd /home/runner/actions-runner \
    && curl -fsSL \
       "https://github.com/actions/runner/releases/download/v${RUNNER_VERSION}/actions-runner-linux-x64-${RUNNER_VERSION}.tar.gz" \
       | tar xz \
    && ./bin/installdependencies.sh \
    && chown -R runner:runner /home/runner/actions-runner

USER runner
WORKDIR /home/runner

# Rust + wasm32
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
    | sh -s -- -y --no-modify-path --profile minimal \
    && /home/runner/.cargo/bin/rustup target add wasm32-unknown-unknown \
    && /home/runner/.cargo/bin/rustup component add rustfmt clippy

# trunk (pinned)
RUN curl -fsSL \
    "https://github.com/trunk-rs/trunk/releases/download/v${TRUNK_VERSION}/trunk-x86_64-unknown-linux-gnu.tar.gz" \
    | tar xz -C /home/runner/.cargo/bin/ \
    && chmod +x /home/runner/.cargo/bin/trunk

# wasm-bindgen-cli (pinned)
RUN curl -fsSL \
    "https://github.com/rustwasm/wasm-bindgen/releases/download/${WASM_BINDGEN_VERSION}/wasm-bindgen-${WASM_BINDGEN_VERSION}-x86_64-unknown-linux-musl.tar.gz" \
    | tar xz --strip-components=1 -C /home/runner/.cargo/bin/ \
    && chmod +x /home/runner/.cargo/bin/wasm-bindgen

COPY --chown=runner:runner entrypoint.sh /home/runner/entrypoint.sh
RUN chmod +x /home/runner/entrypoint.sh

ENTRYPOINT ["/home/runner/entrypoint.sh"]
