# syntax=docker/dockerfile:1

# =============================================================================
# Stage 1: Builder
# =============================================================================
FROM rust:1-bookworm AS builder

WORKDIR /app

# Pre-cache dependencies using a dummy main/lib before copying full source
COPY Cargo.toml ./
RUN mkdir -p src && \
    echo "fn main() {}" > src/main.rs && \
    echo "" > src/lib.rs && \
    cargo build --release && \
    rm -rf src

# Copy actual source code and build the final release binary
COPY src ./src
RUN touch src/lib.rs src/main.rs && \
    cargo build --release --locked || cargo build --release

# =============================================================================
# Stage 2: Minimal Runtime Image
# =============================================================================
FROM debian:bookworm-slim AS runtime

RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates && \
    rm -rf /var/lib/apt/lists/* && \
    useradd --system --uid 10001 --create-home --shell /usr/sbin/nologin iopool

WORKDIR /app

COPY --from=builder /app/target/release/iopool2mqtt /usr/local/bin/iopool2mqtt

USER iopool

ENTRYPOINT ["/usr/local/bin/iopool2mqtt"]
