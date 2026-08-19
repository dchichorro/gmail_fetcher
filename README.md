# Gmail Fetcher

Polls Gmail for emails with attachments using configurable filters. Downloads and stores attachments locally with metadata in SQLite.

## Quick Start

```bash
# 1. Copy and edit config
cp .env.example .env
cp filters.toml.example filters.toml
# Edit .env with your Google OAuth2 credentials
# Edit filters.toml with your Gmail search queries

# 2. Run locally
cargo run --release

# 3. Or run with Docker
docker compose up -d
```

## Filters

Filters are defined in `filters.toml`:

```toml
[[filters]]
name = "continente"
gmail_query = "from:noreply@cartaocontinente.pt has:attachment"

[[filters]]
name = "repsol"
gmail_query = "from:noreply@repsol.pt has:attachment"
```

Each filter runs its own Gmail query. Attachments are tagged with the filter name for downstream processing.

## CLI

```
gmail_fetcher poll          # Poll Gmail and fetch attachments (default)
gmail_fetcher summary       # Show receipt summary
gmail_fetcher summary --filter continente  # Summary for one filter
gmail_fetcher filters       # List configured filters
gmail_fetcher chart         # Generate spending chart
gmail_fetcher chart --filter continente --output ./chart.jpg
```

## Architecture

```
┌─────────────┐     ┌──────────────┐     ┌──────────────┐
│   Gmail     │────▶│ gmail-fetcher│────▶│  SQLite DB   │
│   Push/Poll │     │  (this app)  │     │ attachments  │
└─────────────┘     └──────────────┘     └──────┬───────┘
                                                 │
                                          ┌──────▼───────┐
                                          │ receipt-     │
                                          │ parser       │
                                          │ (separate)   │
                                          └──────────────┘
```

- **gmail-fetcher**: Fetches emails + attachments, stores metadata. Triggered by Gmail Push or polling.
- **receipt-parser**: Separate service that parses PDFs by configurable rules. Runs on schedule or triggered.

## OAuth2 Setup

1. Go to [Google Cloud Console](https://console.cloud.google.com/apis/credentials)
2. Create OAuth2 credentials (Desktop app)
3. Enable Gmail API
4. Set `CLIENT_ID` and `CLIENT_SECRET` in `.env`
5. Run the app — it will print an auth URL on first run
6. Paste the authorization code when prompted

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `CLIENT_ID` | required | Google OAuth2 client ID |
| `CLIENT_SECRET` | required | Google OAuth2 client secret |
| `FILTERS_CONFIG` | `./filters.toml` | Path to filters config |
| `POLL_INTERVAL` | `60` | Seconds between polls |
| `OUTPUT_DIR` | `./attachments` | Where to save files |
| `TOKEN_FILE` | `./token.json` | OAuth2 token storage |
| `DB_PATH` | `./data/gmail_fetcher.db` | SQLite database path |
| `RUST_LOG` | `info` | Log level |
