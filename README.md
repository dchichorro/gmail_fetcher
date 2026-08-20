# Gmail Fetcher

Polls Gmail for emails with attachments using configurable filters. Downloads and stores attachments locally with metadata in SQLite.

## Quick Start

```bash
# 1. Copy and edit config
cp .env.example .env
# Edit .env with your Google OAuth2 credentials

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

Filters are validated at startup — empty names or queries cause an immediate error.

## CLI

```
gmail_fetcher poll       # Poll Gmail and fetch attachments (default)
gmail_fetcher filters    # List configured filters
```

## Architecture

```
┌─────────────┐     ┌──────────────┐     ┌──────────────┐
│   Gmail     │────▶│ gmail-fetcher│────▶│  SQLite DB   │
│   (Poll)    │     │  (this app)  │     │ attachments  │
└─────────────┘     └──────────────┘     └──────┬───────┘
                                                │
                                         ┌──────▼───────┐
                                         │ receipt-     │
                                         │ parser       │
                                         │ (separate)   │
                                         └──────────────┘
```

- **gmail-fetcher**: Fetches emails + attachments via polling. Stores metadata in SQLite, files on disk organized by `YYYY/MM/DD/<email_id>/`.
- **receipt-parser**: Separate service that parses PDFs by configurable rules. Owns all receipt/invoice parsing logic.

## OAuth2 Setup

1. Go to [Google Cloud Console](https://console.cloud.google.com/apis/credentials)
2. Create OAuth2 credentials (Desktop app)
3. Enable Gmail API
4. Set `CLIENT_ID` and `CLIENT_SECRET` in `.env`
5. Run `python3 auth_helper.py` locally to authorize
6. Copy the generated `token.json` to the server

The daemon handles token refresh automatically. If the refresh token expires, it enters backoff mode and logs a re-auth instruction.

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
| `RUST_LOG` | `info` | Log level (`info` or `debug`) |

## Development

```bash
# Build
cargo build --release

# Run tests (21 tests: unit + integration)
cargo test

# Lint
cargo clippy --all-targets
```

### Project Structure

```
src/
  lib.rs          # Library crate (public API for integration tests)
  main.rs         # Binary entry point, CLI, poll loop
  filters.rs      # Filter config loading and validation
  db.rs           # SQLite schema, migrations, queries
  gmail.rs        # Gmail API client, attachment downloads
  auth.rs         # OAuth2 token management
  constants.rs    # API URLs

tests/
  filters_test.rs           # Filter validation integration tests
  attachment_path_test.rs   # Attachment path + write cycle tests
```
