use anyhow::Result;
use tokio::time::{sleep, Duration};
use reqwest::Client;
use crate::auth::{get_oauth_client, get_access_token};
use crate::gmail::{fetch_emails, download_attachment};
use std::collections::HashSet;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};

mod constants;
mod auth;
mod gmail;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    // Initialize OAuth2 client
    let client = get_oauth_client();

    // Track processed email IDs to avoid reprocessing
    let mut processed_emails: HashSet<String> = HashSet::new();

    // Load the most recent processed email ID from file
    let last_processed_email_id = load_last_processed_email_id()?;

    loop {
        // Get a new access token before polling for emails
        println!("Getting access token...");
        let access_token = match get_access_token(&client).await {
            Ok(token) => token,
            Err(err) => {
                println!("Failed to get access token: {:?}", err);
                sleep(Duration::from_secs(10)).await; // Wait and retry if access token retrieval fails
                continue;
            }
        };

        // Periodically poll Gmail for new emails
        let http_client = Client::new();
        println!("Polling for new emails...");

        let query = if let Some(last_id) = &last_processed_email_id {
            // Use the last processed email ID in the query to get new emails
            format!("from:noreply@cartaocontinente.pt has:attachment newer_than_id:{}", last_id)
        } else {
            // If there's no last ID, fetch all matching emails
            "from:noreply@cartaocontinente.pt has:attachment".to_string()
        };

        match fetch_emails(&http_client, &access_token, &query).await {
            Ok(emails) => {
                for email in emails {
                    if processed_emails.contains(&email.id) {
                        continue;
                    }

                    println!("Found email: {}", email.id);
                    match download_attachment(&http_client, &access_token, &email.id, "./attachments").await {
                        Ok(_) => {
                            // Mark email as processed only if download was successful
                            processed_emails.insert(email.id.clone());
                            save_last_processed_email_id(&email.id)?;
                        }
                        Err(err) => {
                            println!("Failed to download attachment for email {}: {:?}", email.id, err);
                        }
                    }
                }
            }
            Err(err) => {
                println!("Failed to fetch emails: {:?}", err);
                sleep(Duration::from_secs(10)).await; // Wait and retry if fetching emails fails
            }
        }

        // Wait before polling again
        println!("Waiting before polling again...");
        sleep(Duration::from_secs(60)).await;
    }
}

// Load the last processed email ID from file
fn load_last_processed_email_id() -> Result<Option<String>> {
    if let Ok(file) = File::open("last_processed_email_id.txt") {
        let mut reader = BufReader::new(file);
        let mut last_id = String::new();
        if reader.read_line(&mut last_id)? > 0 {
            return Ok(Some(last_id.trim().to_string()));
        }
    }
    Ok(None)
}

// Save the last processed email ID to file
fn save_last_processed_email_id(last_id: &str) -> Result<()> {
    let mut file = File::create("last_processed_email_id.txt")?;
    file.write_all(last_id.as_bytes())?;
    Ok(())
}
