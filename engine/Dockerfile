# Capsuled Engine - Production Dockerfile
# Multi-stage build for minimal runtime image
#
# Build: docker build -t capsuled-engine:latest .
# Run:   docker run -p 4500:4500 capsuled-engine:latest

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

WORKDIR /app

# Copy workspace files
COPY Cargo.toml Cargo.lock ./
COPY build.rs ./
COPY src ./src
COPY migrations ./migrations

# Copy libadep-core dependency
COPY ../libadep /libadep

# Build release binary
RUN cargo build --release --bin capsuled-engine

# ============================================
# Stage 2: Runtime Stage
# ============================================
FROM debian:bookworm-slim AS runtime

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN groupadd -r capsuled && useradd -r -g capsuled capsuled

# Create necessary directories
RUN mkdir -p /var/lib/capsuled /var/log/capsuled /tmp/capsuled/logs /tmp/capsuled/keys \
    && chown -R capsuled:capsuled /var/lib/capsuled /var/log/capsuled /tmp/capsuled

# Copy binary from builder
COPY --from=builder /app/target/release/capsuled-engine /usr/local/bin/

# Copy migrations
COPY --from=builder /app/migrations /var/lib/capsuled/migrations

# Set environment
ENV RUST_LOG=info
ENV CAPSULED_DATA_DIR=/var/lib/capsuled
ENV CAPSULED_LOG_DIR=/var/log/capsuled

# Switch to non-root user
USER capsuled

# Expose ports
# 4500: HTTP API
# 4501: gRPC (future)
EXPOSE 4500 4501

# Health check
HEALTHCHECK --interval=30s --timeout=3s --start-period=10s --retries=3 \
    CMD curl -f http://localhost:4500/health || exit 1

# Default command
ENTRYPOINT ["capsuled-engine"]
CMD ["--port", "4500"]
