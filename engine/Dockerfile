# Capsuled Engine - Production Dockerfile
# Multi-stage build for minimal runtime image
#
# Build context: capsuled/ root
# Build: docker build -f engine/Dockerfile -t capsuled-engine:latest .

# ============================================
# Stage 1: Build Stage (Rust)
# ============================================
FROM rust:1.83-bookworm AS builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    protobuf-compiler \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /workspace

# Copy entire workspace
COPY . .

WORKDIR /workspace/engine

# Build release binary
RUN cargo build --release --bin capsuled-engine

# ============================================
# Stage 2: Runtime Stage
# ============================================
FROM debian:bookworm-slim AS runtime

# Install runtime dependencies + Docker CLI (for DooD pattern)
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    curl \
    netcat-openbsd \
    gnupg \
    && rm -rf /var/lib/apt/lists/*

# Add Docker's official GPG key and repository
RUN install -m 0755 -d /etc/apt/keyrings && \
    curl -fsSL https://download.docker.com/linux/debian/gpg -o /etc/apt/keyrings/docker.asc && \
    chmod a+r /etc/apt/keyrings/docker.asc && \
    echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.asc] https://download.docker.com/linux/debian bookworm stable" > /etc/apt/sources.list.d/docker.list && \
    apt-get update && \
    apt-get install -y docker-ce-cli && \
    rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN groupadd -r capsuled && useradd -r -g capsuled capsuled

# Create necessary directories
RUN mkdir -p /var/lib/capsuled /var/log/capsuled /tmp/capsuled/logs /tmp/capsuled/keys \
    && chown -R capsuled:capsuled /var/lib/capsuled /var/log/capsuled /tmp/capsuled

# Copy binary from builder
COPY --from=builder /workspace/target/release/capsuled-engine /usr/local/bin/

# Copy migrations
COPY --from=builder /workspace/engine/migrations /var/lib/capsuled/migrations

# Set environment
ENV RUST_LOG=info
ENV CAPSULED_DATA_DIR=/var/lib/capsuled
ENV CAPSULED_LOG_DIR=/var/log/capsuled

# NOTE: Running as root for Docker socket access (DooD pattern)
# In production, use docker group or rootless Docker
# USER capsuled

# Expose ports
# 50051: gRPC
# 4500: HTTP API
# 4501: Egress Proxy
EXPOSE 50051 4500 4501

# Health check
HEALTHCHECK --interval=30s --timeout=3s --start-period=10s --retries=3 \
    CMD curl -f http://localhost:4500/health || exit 1

# Default command
ENTRYPOINT ["capsuled-engine"]
CMD ["--port", "4500"]
