# === BUILD STAGE ===
FROM rust:1.84-slim-bookworm as builder

WORKDIR /usr/src/app

COPY Cargo.toml Cargo.lock ./

# i've seen this when doing research, apparently it's a trick for caching? i'll revise later, when the app is fully done
RUN mkdir src && echo "fn main() {}" > src/main.rs

RUN cargo build --release

RUN rm -f target/release/deps/myapp*
RUN rm -rf src

COPY . .

# === RUNTIME ===
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y libssl3 ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /usr/src/app/target/release/shadow /usr/local/bin/app

# TODO: parametrize, maybe with docker compose
EXPOSE 9999

CMD ["app"]