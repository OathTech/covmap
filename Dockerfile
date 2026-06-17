# syntax=docker/dockerfile:1

# covmap — multi-stage Dockerfile.
#
# Targets:
#   runtime  (default) — slim image containing just the `covmap` binary, for users.
#   ci                 — full Rust toolchain + rustfmt, clippy, and python3, for the
#                        GitLab pipeline (fmt / clippy / build / test / smoke).
#
# Examples:
#   docker build -t covmap .                          # -> runtime image
#   docker run --rm -v "$PWD":/work -w /work covmap    # run covmap against the cwd
#   docker build --target ci -t covmap-ci .           # -> CI image

ARG RUST_VERSION=1.82
ARG DEBIAN_RELEASE=bookworm

# ---------------------------------------------------------------------------
# builder — compile the release binary.
# ---------------------------------------------------------------------------
FROM rust:${RUST_VERSION}-slim-${DEBIAN_RELEASE} AS builder
WORKDIR /covmap

# Cache the dependency build separately from the source so edits to src/ don't
# force a full recompile of the (single) third-party crate.
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo 'fn main() {}' > src/main.rs \
    && cargo build --release \
    && rm -rf src

COPY src ./src
# Touch so cargo notices the real main.rs is newer than the dummy build above.
RUN touch src/main.rs && cargo build --release

# ---------------------------------------------------------------------------
# ci — full toolchain for the pipeline. Mirrors .gitlab-ci.yml requirements:
#   rustfmt + clippy components, plus python3/bash for smoke_test/run.sh.
# ---------------------------------------------------------------------------
FROM rust:${RUST_VERSION}-slim-${DEBIAN_RELEASE} AS ci
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        python3 \
        bash \
        ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && rustup component add rustfmt clippy
WORKDIR /covmap

# ---------------------------------------------------------------------------
# runtime — minimal image for end users. Last stage, so it is the default
# build target (`docker build` with no --target produces this).
# ---------------------------------------------------------------------------
FROM debian:${DEBIAN_RELEASE}-slim AS runtime
LABEL org.opencontainers.image.title="covmap" \
      org.opencontainers.image.description="Track which parts of a spec are realized by which parts of an implementation." \
      org.opencontainers.image.licenses="Apache-2.0" \
      org.opencontainers.image.source="https://gitlab-ext.galois.com/specfro/covmap"

COPY --from=builder /covmap/target/release/covmap /usr/local/bin/covmap

# Mount your project at /work and run covmap subcommands against it.
WORKDIR /work
ENTRYPOINT ["covmap"]
CMD ["--help"]
