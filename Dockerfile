FROM rust:1.82.0-bullseye AS builder

RUN apt update && apt install cmake -y

WORKDIR /pj
COPY . .


RUN cargo build --release


FROM debian:bullseye-slim

COPY --from=builder /pj/target/release/pj /usr/local/bin/pj

CMD ["pj", "--help"]