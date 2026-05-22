FROM rust:1.94 AS base

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update \
  && apt-get install -y --no-install-recommends \
    curl ca-certificates build-essential \
    libwebkit2gtk-4.1-dev libjavascriptcoregtk-4.1-dev \
    libssl-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev \
  && rm -rf /var/lib/apt/lists/*

# Install bun (installer places binaries in /root/.bun)
RUN curl -fsSL https://bun.sh/install | bash -s -- -y || true

ENV PATH="/root/.bun/bin:/root/.cargo/bin:/usr/local/cargo/bin:${PATH}"

WORKDIR /workspace

# Copy project files and do a frontend build and a release Rust build
COPY . /workspace

RUN if [ -f package.json ]; then \
    /root/.bun/bin/bun install --frozen-lockfile --cache-dir /workspace/.bun-cache && \
    /root/.bun/bin/bun run build ; \
  fi

WORKDIR /workspace/src-tauri

# Build Rust release (cache will be used by the runner/docker layer cache when available)
RUN cargo build --release || true

CMD ["/bin/bash"]
