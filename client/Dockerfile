# --------------------------------------------------------
# Gumball Coordinator - Production Dockerfile
# Uses Debian Bookworm (glibc) for CGO compatibility
# --------------------------------------------------------

# NOTE: This Dockerfile is built by GitHub Actions (build-push workflow) with build context at repo root.
# Do NOT change COPY paths (e.g. `COPY client/...`) unless you also change the build context accordingly.
# This image is published as: ghcr.io/<owner>/gumball-coordinator:<tag>

# 1. Builder Stage (Debian Bookworm with Go 1.24)
FROM golang:1.24-bookworm AS builder

WORKDIR /app

# Install build dependencies for CGO
RUN apt-get update && apt-get install -y \
    gcc \
    libc6-dev \
    && rm -rf /var/lib/apt/lists/*

# Go dependency resolution (cached layer)
COPY client/go.mod client/go.sum ./

# Download dependencies with retry logic for reliability
RUN --mount=type=cache,target=/go/pkg/mod \
    for i in 1 2 3 4 5; do \
        go mod download && break || \
        (echo "Retry $i/5 failed, waiting..." && sleep 10); \
    done && \
    go mod verify

# Copy source and build
# wasmer-go v1.0.4 bundles its own libwasmer, no external install needed
# Extract the bundled Wasmer shared library BEFORE build for linking
# IMPORTANT: If you update wasmer-go, verify the packaged lib paths below.
# A seemingly harmless dependency bump can break runtime linking if libwasmer moves.
RUN set -eux; \
    WASMER_DIR="$(go list -f '{{.Dir}}' github.com/wasmerio/wasmer-go/wasmer)"; \
    GOARCH="$(go env GOARCH)"; \
    case "$GOARCH" in \
        amd64) WASMER_SO="$WASMER_DIR/packaged/lib/linux-amd64/libwasmer.so" ;; \
        arm64) WASMER_SO="$WASMER_DIR/packaged/lib/linux-aarch64/libwasmer.so" ;; \
        *) echo "Unsupported GOARCH for bundled wasmer: $GOARCH" >&2; exit 1 ;; \
    esac; \
    test -n "$WASMER_SO"; \
    test -f "$WASMER_SO"; \
    cp -v "$WASMER_SO" /usr/lib/libwasmer.so

# Copy source and build
COPY client/ .
RUN CGO_ENABLED=1 GOOS=linux go build -ldflags="-s -w" -o /bin/coordinator ./cmd/coordinator

# --------------------------------------------------------
# 2. Runtime Stage (Debian slim - for glibc compatibility)
# --------------------------------------------------------
FROM debian:bookworm-slim AS runtime

# Install minimal runtime dependencies
# Install minimal runtime dependencies + Caddy
# NOTE: We install official Caddy packages for predictable behavior.
# Avoid swapping to Alpine here unless you also validate glibc + libwasmer behavior.
RUN apt-get update && apt-get install -y \
    ca-certificates \
    curl \
    debian-keyring \
    debian-archive-keyring \
    apt-transport-https \
    && curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/gpg.key' | gpg --dearmor -o /usr/share/keyrings/caddy-stable-archive-keyring.gpg \
    && curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/debian.deb.txt' | tee /etc/apt/sources.list.d/caddy-stable.list \
    && apt-get update && apt-get install -y caddy \
    && rm -rf /var/lib/apt/lists/*

# Create app user and data directory
# Ensure Caddy can read config and write to its storage
RUN useradd -r -u 1000 coordinator && \
    mkdir -p /app/data && \
    mkdir -p /etc/caddy && \
    chown -R coordinator:coordinator /app && \
    chown -R coordinator:coordinator /etc/caddy && \
    # Create caddy data dir that coordinator user can access
    mkdir -p /var/lib/caddy /var/log/caddy && \
    chown -R coordinator:coordinator /var/lib/caddy /var/log/caddy

WORKDIR /app

# Copy binary
COPY --from=builder /bin/coordinator /usr/local/bin/coordinator
COPY client/entrypoint.sh /usr/local/bin/entrypoint.sh
COPY client/Caddyfile /etc/caddy/Caddyfile
RUN chmod +x /usr/local/bin/entrypoint.sh

# Copy Wasmer shared library required by wasmer-go
COPY --from=builder /usr/lib/libwasmer.so /usr/local/lib/libwasmer.so

ENV LD_LIBRARY_PATH=/usr/local/lib
ENV XDG_DATA_HOME=/app/data
ENV XDG_CONFIG_HOME=/etc/caddy

# NOTE: If you change these env vars, also update:
# - client/entrypoint.sh (expects writable XDG paths)
# - docker-compose*.yml volume mounts (persist /app/data)

# Expose gRPC (50050), HTTP (8081), and Caddy (80/443/2019)
EXPOSE 50050 8081 80 443 2019

# Run as non-root
USER coordinator

ENTRYPOINT ["/usr/local/bin/entrypoint.sh"]
