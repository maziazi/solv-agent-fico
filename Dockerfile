# Multi-stage build of the one deployable artefact. The frontend dist is
# committed, so no Node toolchain exists in either stage (docs/DESIGN.md §6).
# Every directory include_str!/include_bytes! reads must be copied: src,
# static, templates, frontend/dist (the built SPA) and frontend/public (the
# font licence text embedded next to the fonts).
FROM rust:1-bookworm AS build
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY static ./static
COPY templates ./templates
COPY frontend/dist ./frontend/dist
COPY frontend/public ./frontend/public
RUN cargo build --release --locked

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=build /app/target/release/solv /usr/local/bin/solv
# The state file's default name resolves inside the workdir, so snapshot
# writes land somewhere the unprivileged runtime user can write. A durable
# volume is the eventual answer; until then restarts lose nothing but the
# last snapshot window (SPEC §5.6), and the verify loop cannot mistake a
# read-only root for a healthy deploy because /healthz still answers.
RUN mkdir -p /app && chown 1000:1000 /app
WORKDIR /app
ENV PORT=8080
USER 1000
EXPOSE 8080
CMD ["solv"]
