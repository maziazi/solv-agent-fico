# Multi-stage build of the one deployable artefact. The frontend dist is
# committed, so no Node toolchain exists in either stage (docs/DESIGN.md §6).
FROM rust:1-bookworm AS build
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY static ./static
COPY templates ./templates
COPY frontend/dist ./frontend/dist
RUN cargo build --release --locked

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=build /app/target/release/solv /usr/local/bin/solv
ENV PORT=8080
USER 1000
EXPOSE 8080
CMD ["solv"]
