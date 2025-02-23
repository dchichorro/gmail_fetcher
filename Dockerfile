FROM rust:latest
WORKDIR /app
COPY . .

RUN apt-get update && apt-get install -y pkg-config libssl-dev

RUN cargo build --release

CMD ["./target/release/gmail_fetcher"]
