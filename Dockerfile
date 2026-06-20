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
ENV ARCHIVEOS_CONFIG=/config
ENV ARCHIVEOS_LISTEN=0.0.0.0:8080
EXPOSE 8080
ENTRYPOINT ["archive-os-server"]
