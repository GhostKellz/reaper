# Multi-stage build for optimal size
FROM rust:1.91-slim AS builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    liblua5.4-dev \
    git \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy manifests
COPY Cargo.toml Cargo.lock ./

# Copy source code
COPY src ./src

# Build release binary
RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    libssl3 \
    liblua5.4-0 \
    git \
    pacman \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd -m -s /bin/bash reaper

# Copy binary from builder
COPY --from=builder /app/target/release/reap /usr/local/bin/reap
RUN ln -s /usr/local/bin/reap /usr/local/bin/reaper

# Set up working directory
WORKDIR /home/reaper
USER reaper

# Entry point
ENTRYPOINT ["reap"]
CMD ["--help"]