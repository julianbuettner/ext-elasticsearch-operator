FROM rust:1.76 AS builder

WORKDIR /app
COPY src ./src
COPY Cargo.toml .
COPY Cargo.lock .

RUN cargo build --release

FROM debian:stable-slim

RUN apt-get update && apt-get install -y libssl-dev

WORKDIR /app
COPY --from=builder /app/target/release/ext-elasticsearch-operator ext-elasticsearch-operator

ENV LOGLEVEL=debug

CMD ["/app/ext-elasticsearch-operator"]
