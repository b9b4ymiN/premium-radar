# syntax=docker/dockerfile:1
# recorder-server — headless Lab recorder image.
# Build from the repo root:
#   docker build -t premium-radar-recorder:dev .

FROM rust:1-bookworm AS builder
WORKDIR /app

# Workspace manifests + lock + the server-side crates.
# src-tauri/Cargo.toml comes along as a manifest-only stub: cargo must parse
# every workspace member even when building with -p recorder-server, but the
# tauri sources/deps are never compiled. rust-toolchain.toml must stay OUT of
# the context — it pins a Windows MSVC toolchain that rustup cannot install
# here (enforced by .dockerignore).
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/
COPY src-tauri/Cargo.toml src-tauri/Cargo.toml
# Cargo validates the explicit [lib] target of every member manifest even
# when that member is never built — an empty stub satisfies it without
# copying any GUI sources.
RUN mkdir -p src-tauri/src && printf '' > src-tauri/src/lib.rs

ENV CARGO_TERM_COLOR=never
# recorder-server pulls radar-core with only the `client` feature
# (reqwest + rustls); the `stream` feature (rumqttc/native-tls) is not in the
# graph, so the builder needs no OpenSSL dev packages.

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/app/target,sharing=locked \
    cargo build --release -p recorder-server \
    && cp target/release/recorder-server /usr/local/bin/recorder-server

FROM debian:bookworm-slim AS runtime

# ca-certificates: reqwest needs a root store for HTTPS to Webull.
# tzdata: honors the TZ env var (ENV below, overridable at run time).
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates tzdata \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --uid 10001 --system --no-create-home recorder \
    && mkdir -p /data \
    && chown recorder:recorder /data

ENV TZ=UTC
WORKDIR /data
VOLUME /data
EXPOSE 8787

COPY --from=builder /usr/local/bin/recorder-server /usr/local/bin/recorder-server
USER recorder
ENTRYPOINT ["recorder-server"]
CMD ["run"]

# HTTP probe via bash's built-in /dev/tcp — no curl/wget in the image.
# Targets the /health endpoint of the HTTP API on :8787. Until that API ships
# (parallel task), `run` containers report unhealthy; harmless with
# restart: unless-stopped, or comment this block out.
HEALTHCHECK --interval=60s --timeout=5s --start-period=2m --retries=3 \
    CMD bash -c 'exec 3<>/dev/tcp/127.0.0.1/8787 && printf "GET /health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n" >&3 && head -n1 <&3 | grep -q " 200 "'
