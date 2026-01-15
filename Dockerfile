# === BUILD STAGE ===
FROM rust:alpine AS builder

WORKDIR /usr/src/app

RUN apk add --no-cache musl-dev pkgconfig openssl-dev

COPY . .

RUN cargo test --release
RUN cargo build --release

# === RUNTIME ===
FROM alpine:edge

RUN apk add --no-cache libgcc ca-certificates
RUN addgroup -S shadowgroup && adduser -S shadowuser -G shadowgroup

WORKDIR /home/shadowuser

COPY --from=builder /usr/src/app/target/release/shadow /usr/local/bin/shadow

USER shadowuser

ENV SHADOW_PORT=9999
EXPOSE $SHADOW_PORT

CMD ["shadow"]
