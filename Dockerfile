# Imagen única para ejecutar el visualizador en cualquier sistema con Docker.
#   docker compose up    →    http://localhost:8000

FROM node:22-bookworm-slim AS web
WORKDIR /src
COPY web/package.json web/package-lock.json web/
RUN cd web && npm ci --no-audit --no-fund
COPY schema schema
COPY traces traces
COPY tools tools
COPY web web
RUN cd web && npm run build

FROM rust:1-bookworm AS server
WORKDIR /src
COPY Cargo.toml Cargo.lock rustfmt.toml ./
COPY crates crates
RUN cargo build --release -p pss-server -p pss-tracer

FROM debian:bookworm-slim
# gcc compila los programas de los estudiantes (fase 1); coreutils y grep los usan los ejemplos con exec.
RUN apt-get update \
    && apt-get install -y --no-install-recommends gcc libc6-dev coreutils grep ca-certificates \
    && rm -rf /var/lib/apt/lists/*
RUN useradd --create-home --uid 10001 pss
WORKDIR /app
COPY --from=server /src/target/release/pss-server /src/target/release/pss-tracer /usr/local/bin/
COPY --from=web /src/web/dist /app/web/dist
COPY config /app/config
ENV PSS_HOST=0.0.0.0 PSS_PORT=8000 PSS_WEB_DIST=/app/web/dist PSS_LIMITS=/app/config/limits.toml
USER pss
EXPOSE 8000
CMD ["pss-server"]
