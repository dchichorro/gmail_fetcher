use oauth2::{basic::BasicClient, reqwest::async_http_client, AuthUrl, AuthorizationCode, RefreshToken, Scope, TokenResponse};
use oauth2::{ClientId, ClientSecret, RedirectUrl, TokenUrl};
use crate::constants::{SCOPES, AUTH_URL, TOKEN_URL};
use std::{fs, env};
use serde::{Deserialize, Serialize};
use anyhow::Result;
use chrono::Utc;
use dotenv::dotenv;

#[derive(Debug, Serialize, Deserialize)]
pub struct Token {
    access_token: String,
    refresh_token: String,
    expires_at: i64,
}

pub fn get_client_id() -> String {
    dotenv().ok();
    env::var("CLIENT_ID").expect("CLIENT_ID must be set")
}

pub fn get_client_secret() -> String {
    dotenv().ok();
    env::var("CLIENT_SECRET").expect("CLIENT_SECRET must be set")
}


fn load_token() -> Option<Token> {
    let token_data = fs::read_to_string("token.json").ok()?;
    serde_json::from_str(&token_data).ok()
}

fn save_token(token: &Token) -> Result<()> {
    let token_data = serde_json::to_string(token)?;
    fs::write("token.json", token_data)?;
    Ok(())
}

// Get an access token by either loading, refreshing, or re-authenticating
pub async fn get_access_token(client: &BasicClient) -> Result<String> {
    // Attempt to load an existing token
    if let Some(mut token) = load_token() {
        let now = Utc::now().timestamp();
        if now < token.expires_at {
            return Ok(token.access_token.clone());
        }

        // If the token is expired, attempt to refresh it
        println!("Refreshing token...");
        let token_result = client
            .exchange_refresh_token(&RefreshToken::new(token.refresh_token.clone()))
            .request_async(async_http_client)
            .await;

        match token_result {
            Ok(token_response) => {
                token.access_token = token_response.access_token().secret().to_string();
                token.expires_at = now + token_response.expires_in().unwrap().as_secs() as i64;
                save_token(&token)?;
                return Ok(token.access_token.clone());
            }
            Err(err) => {
                println!("Failed to refresh token: {:?}", err);
            }
        }
    }

    // No valid token available, start new authentication flow
    println!("No valid token found. Starting new authentication...");

    let (auth_url, _csrf_token) = client
        .authorize_url(oauth2::CsrfToken::new_random)
        .add_scope(Scope::new(SCOPES.to_string()))
        .url();

    // Provide the URL to the user for authorization
    println!("Authorize app at: {}", auth_url);
    println!("Enter the authorization code: ");
    let mut code = String::new();
    std::io::stdin().read_line(&mut code)?;
    let code = code.trim();

    let token_response = client
        .exchange_code(AuthorizationCode::new(code.to_string()))
        .request_async(async_http_client)
        .await?;

    let new_token = Token {
        access_token: token_response.access_token().secret().to_string(),
        refresh_token: token_response
            .refresh_token()
            .ok_or_else(|| anyhow::anyhow!("No refresh token received"))?
            .secret()
            .to_string(),
        expires_at: Utc::now().timestamp() + token_response.expires_in().unwrap().as_secs() as i64,
    };

    save_token(&new_token)?;
    Ok(new_token.access_token)
}



pub fn get_oauth_client() -> BasicClient {
    BasicClient::new(
        ClientId::new(get_client_id().to_string()),
        Some(ClientSecret::new(get_client_secret().to_string())),
        AuthUrl::new(AUTH_URL.to_string()).unwrap(),
        Some(TokenUrl::new(TOKEN_URL.to_string()).unwrap()),
    )
    .set_redirect_uri(RedirectUrl::new("urn:ietf:wg:oauth:2.0:oob".to_string()).unwrap())
}
