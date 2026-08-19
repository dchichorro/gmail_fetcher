use anyhow::{anyhow, Result};
use chrono::Utc;
use dotenvy::dotenv;
use log::{debug, info};
use oauth2::basic::BasicClient;
use oauth2::{
    AuthUrl, ClientId, ClientSecret, EndpointSet, RedirectUrl,
    RefreshToken, TokenResponse, TokenUrl,
};
use crate::constants::{AUTH_URL, TOKEN_URL};
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;

pub type GmailOAuthClient = BasicClient<
    EndpointSet,
    oauth2::EndpointNotSet,
    oauth2::EndpointNotSet,
    oauth2::EndpointNotSet,
    EndpointSet,
>;

#[derive(Debug, Serialize, Deserialize)]
pub struct Token {
    access_token: String,
    refresh_token: String,
    expires_at: i64,
}

fn get_client_id() -> String {
    dotenv().ok();
    env::var("CLIENT_ID").expect("CLIENT_ID must be set")
}

fn get_client_secret() -> String {
    dotenv().ok();
    env::var("CLIENT_SECRET").expect("CLIENT_SECRET must be set")
}

fn load_token(token_path: &str) -> Option<Token> {
    let token_data = fs::read_to_string(token_path).ok()?;
    serde_json::from_str(&token_data).ok()
}

fn save_token(token: &Token, token_path: &str) -> Result<()> {
    let token_data = serde_json::to_string(token)?;
    fs::write(token_path, token_data)?;
    Ok(())
}

pub async fn get_access_token(
    client: &GmailOAuthClient,
    token_path: &str,
) -> Result<String> {
    let http_client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()?;

    if let Some(mut token) = load_token(token_path) {
        let now = Utc::now().timestamp();
        if now < token.expires_at {
            debug!("Using cached access token");
            return Ok(token.access_token.clone());
        }

        info!("Refreshing expired access token");
        let token_result = client
            .clone()
            .exchange_refresh_token(&RefreshToken::new(token.refresh_token.clone()))
            .request_async(&http_client)
            .await;

        match token_result {
            Ok(token_response) => {
                token.access_token = token_response.access_token().secret().to_string();
                token.expires_at =
                    now + token_response.expires_in().unwrap().as_secs() as i64;
                save_token(&token, token_path)?;
                info!("Token refreshed successfully");
                return Ok(token.access_token.clone());
            }
            Err(err) => {
                return Err(anyhow!(
                    "Token refresh failed (re-auth needed): {:?}",
                    err
                ));
            }
        }
    }

    Err(anyhow!(
        "No valid token found. Run: python3 auth_helper.py"
    ))
}

pub fn get_oauth_client() -> GmailOAuthClient {
    BasicClient::new(ClientId::new(get_client_id()))
        .set_client_secret(ClientSecret::new(get_client_secret()))
        .set_auth_uri(AuthUrl::new(AUTH_URL.to_string()).unwrap())
        .set_token_uri(TokenUrl::new(TOKEN_URL.to_string()).unwrap())
        .set_redirect_uri(RedirectUrl::new("urn:ietf:wg:oauth:2.0:oob".to_string()).unwrap())
}
