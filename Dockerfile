# === BUILD STAGE ===
FROM rust:alpine AS builder

WORKDIR /usr/src/app

RUN apk add --no-cache musl-dev pkgconfig openssl-dev cmake make git build-base linux-headers ca-certificates meson

RUN git clone https://github.com/ENIX1701/GHOST /usr/src/GHOST

COPY . .

RUN cargo test --release
RUN cargo build --release

# === RUNTIME ===
FROM alpine:edge

RUN apk add --no-cache libgcc ca-certificates g++ cmake make git linux-headers ccache curl-dev
RUN addgroup -S shadowgroup && adduser -S shadowuser -G shadowgroup

RUN mkdir -p /home/shadowuser/.ccache && chown -R shadowuser:shadowgroup /home/shadowuser/.ccache

ENV CCACHE_DIR=/home/shadowuser/.ccache
ENV PATH="/usr/lib/ccache/bin:$PATH"

WORKDIR /home/shadowuser
 
RUN mkdir -p /home/shadowuser/builds && chown -R shadowuser:shadowgroup /home/shadowuser/builds

RUN git clone https://github.com/ENIX1701/GHOST /usr/src/GHOST

COPY --from=builder /usr/src/app/target/release/shadow /usr/local/bin/shadow

USER shadowuser

ENV GHOST_SOURCE_PATH=/usr/src/GHOST
ENV SHADOW_PORT=9999
EXPOSE $SHADOW_PORT

CMD ["shadow"]
