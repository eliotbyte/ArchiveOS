FROM node:22-bookworm AS web-builder
WORKDIR /web
COPY web/package.json web/package-lock.json ./
RUN npm ci
COPY web/ ./
ARG VITE_VAULT_NAME=default
ARG VITE_API_BASE=
ENV VITE_VAULT_NAME=${VITE_VAULT_NAME}
ENV VITE_API_BASE=${VITE_API_BASE}
RUN npm run build

FROM rust:1-bookworm AS builder
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY contract ./contract
COPY core ./core
COPY app ./app
COPY server ./server
RUN cargo build --release -p archiveos-server

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/archive-os-server /usr/local/bin/archive-os-server
COPY --from=web-builder /web/dist /usr/share/archiveos/web
ENV ARCHIVEOS_CONFIG=/config
ENV ARCHIVEOS_LISTEN=0.0.0.0:8080
ENV ARCHIVEOS_WEB_DIR=/usr/share/archiveos/web
EXPOSE 8080
ENTRYPOINT ["archive-os-server"]
