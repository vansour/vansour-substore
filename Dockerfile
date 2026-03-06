FROM ghcr.io/vansour/rust:trixie AS builder
WORKDIR /app

# --- 优化层：缓存依赖构建 ---
# 创建一个空的 dummy 项目
RUN mkdir src && echo "fn main() {}" > src/main.rs
# 仅复制依赖描述文件
COPY Cargo.toml Cargo.lock ./
# 编译依赖（这一步会被 Docker 缓存，除非 Cargo.toml 变动）
RUN cargo build --release

# --- 源码构建层 ---
# 删除 dummy 源码
RUN rm -rf src
# 复制真实源码和 web 目录
COPY src ./src
COPY web ./web
# 更新文件时间戳，强制 cargo 重新编译 main 包
RUN touch src/main.rs
# 编译实际项目
RUN cargo build --release && strip target/release/vss-substore

FROM ghcr.io/vansour/debian:trixie-slim
# 安装运行时依赖
RUN apt update && \
    DEBIAN_FRONTEND=noninteractive apt install -y --no-install-recommends ca-certificates curl tzdata && \
    apt clean && \
    rm -rf /var/lib/apt/lists/* /tmp/* /var/tmp/*
ENV TZ=Asia/Shanghai
RUN ln -snf /usr/share/zoneinfo/Asia/Shanghai /etc/localtime && echo "Asia/Shanghai" > /etc/timezone

WORKDIR /app
RUN mkdir -p /app/data /app/logs

# 从 builder 阶段复制编译好的二进制文件
COPY --from=builder /app/target/release/vss-substore /app/vss-substore
COPY --from=builder /app/web /app/web

ENV RUST_LOG=info
ENV RUST_BACKTRACE=1

HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
    CMD /usr/bin/curl -f http://localhost:8080/healthz || exit 1

EXPOSE 8080
CMD ["/app/vss-substore"]
