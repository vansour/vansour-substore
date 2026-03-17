FROM ghcr.io/vansour/rust:trixie AS builder
WORKDIR /app

RUN rustup target add wasm32-unknown-unknown
RUN cargo install dioxus-cli --version 0.7.3

COPY . .

RUN dx build --platform web --package submora-web --release
RUN cargo build --release -p submora && strip target/release/submora

FROM ghcr.io/vansour/debian:trixie-slim

RUN apt update && \
    DEBIAN_FRONTEND=noninteractive apt install -y --no-install-recommends ca-certificates curl tzdata sqlite3 && \
    apt clean && \
    rm -rf /var/lib/apt/lists/* /tmp/* /var/tmp/*

ENV TZ=Asia/Shanghai \
    RUST_LOG=info \
    RUST_BACKTRACE=1 \
    HOST=0.0.0.0 \
    PORT=8080 \
    WEB_DIST_DIR=/app/dist \
    DATABASE_URL=sqlite:///app/data/substore.db?mode=rwc

RUN ln -snf /usr/share/zoneinfo/Asia/Shanghai /etc/localtime && echo "Asia/Shanghai" > /etc/timezone

WORKDIR /app
RUN mkdir -p /app/data /app/dist && chmod 777 /app/data

COPY --from=builder /app/target/release/submora /app/submora
COPY --from=builder /app/dist /app/dist

HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
    CMD /usr/bin/curl -f http://localhost:8080/healthz || exit 1

EXPOSE 8080
CMD ["/app/submora"]
