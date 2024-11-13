FROM rust:1.76-slim as builder

WORKDIR /usr/src/app
COPY . .

RUN cargo build --release

FROM debian:bookworm-slim
WORKDIR /usr/local/bin
COPY --from=builder /usr/src/app/target/release/rust-ws .

EXPOSE 8000

CMD ["./rust-ws"] 