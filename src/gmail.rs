use anyhow::{anyhow, Result};
use base64::{decode_config, URL_SAFE};
use reqwest::{Client, Response};
use serde::Deserialize;
use serde_json::Value;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use tokio::time::{sleep, Duration};

#[derive(Debug, Deserialize)]
pub struct Message {
    pub id: String,
    // thread_id: String,
}

#[derive(Debug, Deserialize)]
struct MessagesResponse {
    messages: Option<Vec<Message>>,
}

pub async fn fetch_emails(
    client: &Client,
    access_token: &str,
    query: &str,
) -> Result<Vec<Message>> {
    let mut all_messages = Vec::new();

    // Build the URL for fetching messages
    let url = format!(
        "https://gmail.googleapis.com/gmail/v1/users/me/messages?q={}",
        query
    );

    // Make the API request
    let resp: Response = client.get(&url).bearer_auth(access_token).send().await?;

    if resp.status().is_success() {
        let messages_response: MessagesResponse = resp.json().await?;
        if let Some(messages) = messages_response.messages {
            println!("Query returned {} emails.", messages.len());
            all_messages.extend(messages);
        } else {
            println!("No new emails.");
        }
    } else {
        return Err(anyhow!("Failed to fetch emails: {}", resp.text().await?));
    }

    Ok(all_messages)
}

pub async fn download_attachment(
    client: &Client,
    access_token: &str,
    message_id: &str,
    output_dir: &str,
) -> Result<()> {
    let url = format!(
        "https://gmail.googleapis.com/gmail/v1/users/me/messages/{}?format=full",
        message_id
    );

    let resp: Response = client.get(&url).bearer_auth(access_token).send().await?;

    if resp.status().is_success() {
        println!("Downloading attachment for email: {}", message_id);
        let email: Value = resp.json().await?;

        // Function to recursively find attachments in nested parts
        fn extract_attachments(part: &Value) -> Vec<(&str, &str)> {
            let mut attachments = Vec::new();

            if let Some(parts) = part["parts"].as_array() {
                for sub_part in parts {
                    attachments.extend(extract_attachments(sub_part));
                }
            } else if let Some(filename) = part["filename"].as_str() {
                if let Some(attachment_id) = part["body"]["attachmentId"].as_str() {
                    attachments.push((filename, attachment_id));
                }
            }

            attachments
        }

        // Extract attachments from the root payload
        let attachments = extract_attachments(&email["payload"]);

        for (filename, attachment_id) in attachments {
            let file_path = format!("{}/{}", output_dir, filename);
            if Path::new(&file_path).exists() {
                println!("Attachment {} already exists. Skipping download.", filename);
                continue;
            }

            println!("Found attachment: {}", filename);
            let attachment_url = format!(
                "https://gmail.googleapis.com/gmail/v1/users/me/messages/{}/attachments/{}",
                message_id, attachment_id
            );

            // Retry logic for fetching the attachment
            let mut retries = 0;
            let max_retries = 5;
            let mut success = false;

            while retries < max_retries && !success {
                let attachment_resp: Response = client
                    .get(&attachment_url)
                    .bearer_auth(access_token)
                    .send()
                    .await?;

                if attachment_resp.status().is_success() {
                    println!("Attachment response is successful for {}", filename);
                    let attachment: Value = attachment_resp.json().await?;
                    if let Some(data) = attachment["data"].as_str() {
                        let decoded_data = decode_config(data, URL_SAFE)?;
                        let mut file = File::create(&file_path)?;
                        file.write_all(&decoded_data)?;
                        println!("Saved attachment: {}", filename);
                    } else {
                        println!("No data found in attachment for {}", filename);
                    }
                    success = true;
                } else if attachment_resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
                    retries += 1;
                    let retry_after = retries * 5; // Exponential back-off: wait longer for each retry
                    println!(
                        "Rate limit hit for attachment {}. Retrying after {} seconds...",
                        filename, retry_after
                    );
                    sleep(Duration::from_secs(retry_after)).await;
                } else {
                    println!(
                        "Failed to fetch attachment {}: {}",
                        filename,
                        attachment_resp
                            .text()
                            .await
                            .unwrap_or_else(|_| "No response text".to_string())
                    );
                    retries += 1;
                    let retry_after = retries * 2; // Basic back-off for other failures
                    println!("Retrying after {} seconds...", retry_after);
                    sleep(Duration::from_secs(retry_after)).await;
                }
            }

            if !success {
                println!(
                    "Failed to download attachment {} after {} attempts. Skipping...",
                    filename, max_retries
                );
            }
        }

        Ok(())
    } else {
        Err(anyhow!("Failed to fetch email: {}", resp.text().await?))
    }
}
