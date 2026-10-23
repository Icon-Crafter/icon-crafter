# syntax=docker/dockerfile:1

# Build stage: compile the `iconcrafter-server` binary from the workspace.
FROM rust:1.98-bookworm AS builder

WORKDIR /build

# `ring`/`rustls` and sqlx need a C toolchain only for build scripts; the
# bookworm image already carries gcc, so no extra packages are required here.
COPY rust-toolchain.toml Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY apps/backend ./apps/backend
COPY apps/desktop ./apps/desktop

# `--locked` keeps the build reproducible against the committed Cargo.lock.
RUN cargo build --release --locked -p iconcrafter-server

# Runtime stage: minimal image with CA roots (TLS to Postgres/S3) and curl for
# the container healthcheck.
FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --create-home --uid 10001 iconcrafter

WORKDIR /app

COPY --from=builder /build/target/release/iconcrafter-server /usr/local/bin/iconcrafter-server

# The migrations are embedded into the binary with `sqlx::migrate!`, so no
# migration files are copied here. Local storage, when STORAGE_KIND=local,
# writes beneath /app/data/storage (mount a volume there).
RUN mkdir -p /app/data/storage && chown -R iconcrafter:iconcrafter /app

USER iconcrafter

ENV LISTEN_ADDR=0.0.0.0:8080 \
    STORAGE_LOCAL_DIR=/app/data/storage

EXPOSE 8080

HEALTHCHECK --interval=30s --timeout=5s --start-period=20s --retries=3 \
    CMD curl -fsS http://127.0.0.1:8080/healthz || exit 1

ENTRYPOINT ["iconcrafter-server"]
