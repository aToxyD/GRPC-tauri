FROM rust:1.94 AS base

ENV DEBIAN_FRONTEND=noninteractive

# Install system dependencies
RUN apt-get update \
  && apt-get install -y --no-install-recommends \
    curl ca-certificates build-essential \
    libwebkit2gtk-4.1-dev libjavascriptcoregtk-4.1-dev \
    libssl-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev \
  && rm -rf /var/lib/apt/lists/*

# Install pinned bun version for reproducibility
RUN curl -fsSL https://bun.sh/install | bash -s "bun-v1.3.9" \
  && /root/.bun/bin/bun --version

ENV PATH="/root/.bun/bin:/root/.cargo/bin:/usr/local/cargo/bin:${PATH}"

# Pre-cache Rust toolchain components used in CI
RUN rustup component add rustfmt clippy

WORKDIR /workspace

# --- Layer 1: Install JS dependencies (cached unless bun.lock changes) ---
COPY package.json bun.lock ./
RUN bun install --frozen-lockfile --cache-dir /workspace/.bun-cache

# --- Layer 2: Copy remaining source and build frontend ---
COPY . /workspace
RUN bun run build

# --- Layer 3: Pre-cache Rust dependencies (cached unless Cargo.toml/Cargo.lock changes) ---
WORKDIR /workspace/src-tauri
RUN cargo fetch

# Pre-compile dependencies only (not the application) for faster CI runs
RUN cargo build 2>&1 | head -200 || true

CMD ["/bin/bash"]
