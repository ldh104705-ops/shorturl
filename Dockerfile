# syntax=docker/dockerfile:1

# ---------- 1단계: 빌드 ----------
# 공식 dx 바이너리가 glibc 2.39 이상을 요구하므로 Debian 13(trixie) 기반을 쓴다.
FROM rust:1.99.1-trixie AS builder

# 브라우저용 wasm 타깃
RUN rustup target add wasm32-unknown-unknown

# Dioxus CLI. Cargo.toml의 dioxus 버전(0.7.10)과 맞춘다.
# 소스 컴파일 대신 공식 릴리스 바이너리를 받아 SHA256을 검증한다.
ARG DX_VERSION=0.7.10
ARG DX_SHA256=4363e4ed2a3f1eb7f4d38d2d59aed59ce43271c44c16b425e92c89a64761fbe7
RUN curl -fsSL -o /tmp/dx.tar.gz \
      "https://github.com/DioxusLabs/dioxus/releases/download/v${DX_VERSION}/dx-x86_64-unknown-linux-gnu.tar.gz" \
 && echo "${DX_SHA256}  /tmp/dx.tar.gz" | sha256sum -c - \
 && tar -xzf /tmp/dx.tar.gz -C /usr/local/bin dx \
 && rm /tmp/dx.tar.gz \
 && dx --version

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY assets ./assets
COPY migrations ./migrations

# 서버 실행 파일(server) + public 폴더(wasm/JS/CSS)를 함께 만든다.
# 결과: target/dx/shorturl/release/web/{server, public/}
RUN dx bundle --platform web --fullstack --release \
 && mkdir -p /out \
 && cp -r target/dx/shorturl/release/web/. /out/ \
 && ls -la /out

# ---------- 2단계: 실행 ----------
# 빌드 단계와 같은 Debian 버전(glibc)을 맞춘다.
FROM debian:trixie-slim AS runtime

RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --system --no-create-home app

WORKDIR /app
# 서버는 실행 파일 옆의 public 폴더에서 정적 파일을 찾는다.
COPY --from=builder /out/server ./server
COPY --from=builder /out/public ./public

# Dioxus 서버는 IP/PORT 환경변수로 바인딩 주소를 정한다(기본값 127.0.0.1:8080).
# 컨테이너 밖에서 접속할 수 있게 0.0.0.0에 바인딩하고, Railway가 PORT를 주면 그 값을 쓴다.
ENV IP=0.0.0.0 \
    PORT=8080
EXPOSE 8080

USER app
CMD ["./server"]
