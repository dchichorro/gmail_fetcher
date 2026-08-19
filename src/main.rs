use anyhow::Result;
use clap::{Parser, Subcommand};
use log::{debug, error, info};
use reqwest::Client;
use tokio::time::{sleep, Duration};

use crate::auth::{get_access_token, get_oauth_client};
use crate::db::Database;
use crate::gmail::{download_attachments, fetch_emails};
use gmail_fetcher::filters::{load_filters, FilterDef};

mod auth;
mod constants;
mod db;
mod gmail;

const BACKOFF_SECONDS: u64 = 300; // 5 minutes

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AuthState {
    Ok,
    Failed,
}

fn is_auth_error(err: &anyhow::Error) -> bool {
    let msg = err.to_string();
    msg.contains("re-auth needed") || msg.contains("No valid token found")
}

#[derive(Parser, Debug)]
#[command(
    name = "gmail_fetcher",
    about = "Fetches Gmail attachments for configurable filters"
)]
struct Cli {
    /// Path to the filters config file
    #[arg(long, env = "FILTERS_CONFIG", default_value = "./filters.toml")]
    filters_config: String,

    /// Poll interval in seconds
    #[arg(long, env = "POLL_INTERVAL", default_value_t = 60)]
    poll_interval: u64,

    /// Directory to save attachments
    #[arg(long, env = "OUTPUT_DIR", default_value = "./attachments")]
    output_dir: String,

    /// Path to the token file
    #[arg(long, env = "TOKEN_FILE", default_value = "./token.json")]
    token_file: String,

    /// Path to the SQLite database file
    #[arg(long, env = "DB_PATH", default_value = "./data/gmail_fetcher.db")]
    db_path: String,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Poll Gmail and fetch attachments (default if no subcommand given)
    Poll,
    /// List configured filters
    Filters,
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "info");
    }
    env_logger::init();

    let cli = Cli::parse();
    let filters = load_filters(&cli.filters_config)?;
    let poll_interval = cli.poll_interval;
    let output_dir = cli.output_dir;
    let token_file = cli.token_file;
    let db_path = cli.db_path;
    let command = cli.command.unwrap_or(Command::Poll);

    info!(
        "Starting gmail_fetcher v{} with {} filter(s)",
        env!("CARGO_PKG_VERSION"),
        filters.len()
    );

    let db = Database::open(&db_path)?;
    let (email_count, attachment_count) = db.get_stats()?;
    info!(
        "Database loaded: {} emails, {} attachments tracked",
        email_count, attachment_count
    );

    match command {
        Command::Poll => run_poll(&token_file, &output_dir, poll_interval, &db, &filters).await?,
        Command::Filters => run_filters(&filters),
    }

    Ok(())
}

fn run_filters(filters: &[FilterDef]) {
    println!("Configured filters ({}):", filters.len());
    for f in filters {
        println!("  {} — {}", f.name, f.gmail_query);
    }
}

async fn run_poll(token_file: &str, output_dir: &str, poll_interval: u64, db: &Database, filters: &[FilterDef]) -> Result<()> {
    let oauth_client = get_oauth_client();
    let mut auth_state = AuthState::Ok;

    loop {
        debug!("Obtaining access token...");
        let access_token = match get_access_token(&oauth_client, token_file).await {
            Ok(token) => {
                if auth_state == AuthState::Failed {
                    info!("Auth recovered successfully");
                }
                auth_state = AuthState::Ok;
                token
            }
            Err(err) => {
                if is_auth_error(&err) {
                    if auth_state == AuthState::Ok {
                        error!(
                            "AUTH FAILURE: {}. Entering backoff mode (retrying every {}s).\n\
                             To fix, run: python3 auth_helper.py",
                            err, BACKOFF_SECONDS
                        );
                        auth_state = AuthState::Failed;
                    }
                    sleep(Duration::from_secs(BACKOFF_SECONDS)).await;
                    continue;
                }
                error!("Failed to get access token (non-auth): {:?}", err);
                sleep(Duration::from_secs(10)).await;
                continue;
            }
        };

        let http_client = Client::new();

        for filter in filters {
            debug!("[{}] Polling Gmail with query: {}", filter.name, filter.gmail_query);

            match fetch_emails(&http_client, &access_token, &filter.gmail_query).await {
                Ok(emails) => {
                    let mut new_count = 0;
                    for email in &emails {
                        if db.email_exists(&email.id)? {
                            continue;
                        }

                        new_count += 1;
                        debug!("[{}] Processing email: {}", filter.name, email.id);
                        match download_attachments(
                            &http_client,
                            &access_token,
                            &email.id,
                            output_dir,
                            db,
                            &filter.name,
                        )
                        .await
                        {
                            Ok(_) => {
                                info!("[{}] Processed email: {}", filter.name, email.id);
                            }
                            Err(err) => {
                                error!(
                                    "[{}] Failed to process email {}: {:?}",
                                    filter.name, email.id, err
                                );
                            }
                        }
                    }
                    if new_count > 0 {
                        let (ec, ac) = db.get_stats()?;
                        info!(
                            "[{}] Processed {} new email(s). Totals: {} emails, {} attachments",
                            filter.name, new_count, ec, ac
                        );
                    } else {
                        debug!("[{}] No new emails", filter.name);
                    }
                }
                Err(err) => {
                    error!("[{}] Failed to fetch emails: {:?}", filter.name, err);
                    sleep(Duration::from_secs(10)).await;
                }
            }
        }

        debug!(
            "Waiting {} seconds before next poll...",
            poll_interval
        );
        sleep(Duration::from_secs(poll_interval)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_auth_error_detects_refresh_failure() {
        let err = anyhow::anyhow!("Token refresh failed (re-auth needed): some error");
        assert!(is_auth_error(&err));
    }

    #[test]
    fn is_auth_error_detects_no_token() {
        let err = anyhow::anyhow!("No valid token found. Run: python3 auth_helper.py");
        assert!(is_auth_error(&err));
    }

    #[test]
    fn is_auth_error_ignores_other_errors() {
        let err = anyhow::anyhow!("Network error: connection refused");
        assert!(!is_auth_error(&err));
    }

    #[test]
    fn is_auth_error_ignores_generic_auth_text() {
        // Should not match just any error with "auth" in it
        let err = anyhow::anyhow!("Authentication server timed out");
        assert!(!is_auth_error(&err));
    }
}
