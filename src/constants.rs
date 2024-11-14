  use std::env;
  use dotenv::dotenv;

  dotenv().ok();

  pub fn get_client_id() -> String {
      env::var("CLIENT_ID").expect("CLIENT_ID must be set")
  }

  pub fn get_client_secret() -> String {
      env::var("CLIENT_SECRET").expect("CLIENT_SECRET must be set")
  }

pub const SCOPES: &str = "https://www.googleapis.com/auth/gmail.readonly";
pub const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
pub const AUTH_URL: &str = "https://accounts.google.com/o/oauth2/auth";
