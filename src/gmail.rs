use anyhow::{anyhow, Result};
use base64::Engine;
use chrono::{NaiveDate, Utc};
use log::{debug, info, warn};
use reqwest::Client;
use serde::Deserialize;
use serde_json::Value;
use std::path::PathBuf;
use tokio::time::{sleep, Duration};

use crate::db::Database;

pub fn attachment_path(base_dir: &str, date: NaiveDate, email_id: &str, filename: &str) -> PathBuf {
    PathBuf::from(base_dir)
        .join(date.format("%Y").to_string())
        .join(date.format("%m").to_string())
        .join(date.format("%d").to_string())
        .join(email_id)
        .join(filename)
}

#[derive(Debug, Deserialize)]
pub struct Message {
    pub id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MessagesResponse {
    messages: Option<Vec<Message>>,
    next_page_token: Option<String>,
}

pub struct EmailMetadata {
    pub id: String,
    pub subject: String,
    pub sender: String,
    pub date: NaiveDate,
}

pub async fn fetch_emails(
    client: &Client,
    access_token: &str,
    query: &str,
) -> Result<Vec<Message>> {
    let mut all_messages = Vec::new();
    let mut page_token: Option<String> = None;

    loop {
        let mut url = format!(
            "https://gmail.googleapis.com/gmail/v1/users/me/messages?q={}",
            query
        );
        if let Some(ref token) = page_token {
            url = format!("{}&pageToken={}", url, token);
        }

        debug!(
            "GET {} (page_token: {:?})",
            url.split('&').next().unwrap_or(&url),
            page_token.as_deref().unwrap_or("none")
        );
        let resp = client.get(&url).bearer_auth(access_token).send().await?;
        debug!("Response status: {}", resp.status());

        if resp.status().is_success() {
            let messages_response: MessagesResponse = resp.json().await?;
            if let Some(messages) = messages_response.messages {
                let count = messages.len();
                all_messages.extend(messages);
                debug!("Page returned {} messages", count);
            } else if all_messages.is_empty() {
                info!("No emails match the query.");
                return Ok(all_messages);
            }

            if let Some(next_token) = messages_response.next_page_token {
                debug!("Next page token: {}", next_token);
                page_token = Some(next_token);
            } else {
                break;
            }
        } else {
            return Err(anyhow!(
                "Failed to fetch emails: {}",
                resp.text().await?
            ));
        }
    }

    info!("Total emails found: {}", all_messages.len());
    Ok(all_messages)
}

pub fn extract_metadata(email: &Value) -> EmailMetadata {
    let id = email["id"]
        .as_str()
        .unwrap_or("unknown")
        .to_string();

    let mut subject = String::new();
    let mut sender = String::new();
    let mut date_str = String::new();

    if let Some(headers) = email["payload"]["headers"].as_array() {
        for header in headers {
            match header["name"].as_str() {
                Some("Subject") => {
                    subject = header["value"].as_str().unwrap_or("").to_string();
                }
                Some("From") => {
                    sender = header["value"].as_str().unwrap_or("").to_string();
                }
                Some("Date") => {
                    date_str = header["value"].as_str().unwrap_or("").to_string();
                }
                _ => {}
            }
        }
    }

    let cleaned_date = date_str
        .rsplit_once(char::is_whitespace)
        .map(|(rest, tz)| {
            if tz.starts_with('(') {
                rest.trim_end().to_string()
            } else {
                date_str.clone()
            }
        })
        .unwrap_or_else(|| date_str.clone());

    let date = chrono::DateTime::parse_from_str(
        &cleaned_date,
        "%a, %d %b %Y %T %z",
    )
    .or_else(|_| chrono::DateTime::parse_from_str(&cleaned_date, "%d %b %Y %T %z"))
    .map(|dt| dt.date_naive())
    .unwrap_or_else(|_| {
        debug!("Failed to parse date: '{}' (cleaned: '{}'), using today", date_str, cleaned_date);
        Utc::now().date_naive()
    });

    EmailMetadata {
        id,
        subject,
        sender,
        date,
    }
}

pub async fn download_attachments(
    client: &Client,
    access_token: &str,
    message_id: &str,
    base_output_dir: &str,
    db: &Database,
    filter_name: &str,
) -> Result<()> {
    let url = format!(
        "https://gmail.googleapis.com/gmail/v1/users/me/messages/{}?format=full",
        message_id
    );

    let resp = client.get(&url).bearer_auth(access_token).send().await?;

    if !resp.status().is_success() {
        return Err(anyhow!(
            "Failed to fetch email: {}",
            resp.text().await?
        ));
    }

    let email: Value = resp.json().await?;
    let meta = extract_metadata(&email);

    info!(
        "Processing email: {} from '{}' dated {} [filter: {}]",
        meta.id, meta.sender, meta.date, filter_name
    );

    db.insert_email(
        &meta.id,
        &meta.subject,
        &meta.sender,
        &meta.date.to_string(),
        filter_name,
    )?;

    fn extract_attachments(part: &Value) -> Vec<(&str, &str)> {
        let mut attachments = Vec::new();

        if let Some(parts) = part["parts"].as_array() {
            for sub_part in parts {
                attachments.extend(extract_attachments(sub_part));
            }
        }

        if let Some(filename) = part["filename"].as_str() {
            if !filename.is_empty() {
                if let Some(attachment_id) = part["body"]["attachmentId"].as_str() {
                    attachments.push((filename, attachment_id));
                }
            }
        }

        attachments
    }

    let attachments = extract_attachments(&email["payload"]);

    if attachments.is_empty() {
        debug!("No attachments in email {}", message_id);
        return Ok(());
    }

    for (filename, attachment_id) in attachments {
        let file_path = attachment_path(base_output_dir, meta.date, &meta.id, filename);
        let file_path_str = file_path.to_str().unwrap();

        if file_path.exists() {
            debug!("Attachment {} already exists, skipping", filename);
            continue;
        }

        let parent = file_path.parent().unwrap();
        std::fs::create_dir_all(parent)?;

        let mime_type = find_mime_type(&email["payload"], filename);

        info!("Downloading attachment: {}", filename);
        let attachment_url = format!(
            "https://gmail.googleapis.com/gmail/v1/users/me/messages/{}/attachments/{}",
            message_id, attachment_id
        );

        let mut retries = 0;
        let max_retries = 5;
        let mut success = false;

        while retries < max_retries && !success {
            debug!("GET attachment {} (attempt {}/{})", attachment_id, retries + 1, max_retries);
            let attachment_resp = client
                .get(&attachment_url)
                .bearer_auth(access_token)
                .send()
                .await?;
            debug!("Attachment response status: {}", attachment_resp.status());

            if attachment_resp.status().is_success() {
                let attachment: Value = attachment_resp.json().await?;
                if let Some(data) = attachment["data"].as_str() {
                    let decoded_data =
                        base64::engine::general_purpose::URL_SAFE.decode(data)?;
                    let file_size = decoded_data.len() as i64;
                    let mut file = std::fs::File::create(&file_path)?;
                    std::io::Write::write_all(&mut file, &decoded_data)?;

                    db.insert_attachment(
                        &meta.id,
                        filename,
                        file_path_str,
                        file_size,
                        &mime_type,
                        filter_name,
                    )?;

                    info!("Saved attachment: {} ({} bytes)", filename, file_size);
                } else {
                    warn!("No data found in attachment for {}", filename);
                }
                success = true;
            } else if attachment_resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
                retries += 1;
                let retry_after = retries * 5;
                warn!(
                    "Rate limit hit for {}. Retrying after {}s...",
                    filename, retry_after
                );
                sleep(Duration::from_secs(retry_after)).await;
            } else {
                warn!(
                    "Failed to fetch attachment {}: {}",
                    filename,
                    attachment_resp
                        .text()
                        .await
                        .unwrap_or_else(|_| "No response text".to_string())
                );
                retries += 1;
                let retry_after = retries * 2;
                sleep(Duration::from_secs(retry_after)).await;
            }
        }

        if !success {
            warn!(
                "Failed to download attachment {} after {} attempts, skipping",
                filename, max_retries
            );
        }
    }

    Ok(())
}

fn find_mime_type(payload: &Value, target_filename: &str) -> String {
    if let Some(parts) = payload["parts"].as_array() {
        for part in parts {
            if let Some(filename) = part["filename"].as_str() {
                if filename == target_filename {
                    return part["mimeType"]
                        .as_str()
                        .unwrap_or("application/octet-stream")
                        .to_string();
                }
            }
            let result = find_mime_type(part, target_filename);
            if result != "application/octet-stream" {
                return result;
            }
        }
    }
    payload["mimeType"]
        .as_str()
        .unwrap_or("application/octet-stream")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[test]
    fn attachment_path_basic() {
        let date = NaiveDate::from_ymd_opt(2025, 3, 15).unwrap();
        let path = attachment_path("/tmp/attachments", date, "abc123", "receipt.pdf");
        assert_eq!(
            path,
            PathBuf::from("/tmp/attachments/2025/03/15/abc123/receipt.pdf")
        );
    }

    #[test]
    fn attachment_path_single_digit_month_day() {
        let date = NaiveDate::from_ymd_opt(2024, 1, 5).unwrap();
        let path = attachment_path("/data", date, "msg_001", "invoice.pdf");
        assert_eq!(
            path,
            PathBuf::from("/data/2024/01/05/msg_001/invoice.pdf")
        );
    }

    #[test]
    fn attachment_path_prevents_overwrite() {
        // Two different emails on the same day with the same filename
        // produce different paths
        let date = NaiveDate::from_ymd_opt(2025, 6, 1).unwrap();
        let path1 = attachment_path("/data", date, "email_A", "file.pdf");
        let path2 = attachment_path("/data", date, "email_B", "file.pdf");
        assert_ne!(path1, path2);
        assert!(path1.starts_with("/data/2025/06/01/email_A"));
        assert!(path2.starts_with("/data/2025/06/01/email_B"));
    }
}
