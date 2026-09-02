# Penlight Dream Box — multi-stage build
FROM rust:1-slim AS builder
WORKDIR /app
COPY Cargo.toml Cargo.lock* ./
COPY src ./src
COPY web ./web
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/penlight-dream-box /usr/local/bin/penlight-dream-box
EXPOSE 8080
ENV HOST=0.0.0.0
CMD ["penlight-dream-box"]
