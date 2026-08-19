FROM rust:1.97 AS builder
WORKDIR /app
COPY . .

RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /app
RUN mkdir -p /app/attachments /app/data
COPY --from=builder /app/target/release/gmail_fetcher .

CMD ["./gmail_fetcher"]
