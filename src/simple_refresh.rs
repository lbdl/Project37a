use serde::Deserialize;
use urlencoding::encode;
use reqwest;
use crate::config::Config;

#[derive(Deserialize, Debug)]
pub struct TokenResponse {
    pub access_token: String,
    pub expires_in: i64,
    pub token_type: String,
}

pub async fn manual_refresh(cfg: &Config) -> Result<TokenResponse, Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();

    let body = format!("client_id={}&client_secret={}&refresh_token={}&grant_type=refresh_token",
                       encode(&cfg.gmail.client_id),
                       encode(&cfg.gmail.client_secret),
                       encode(&cfg.gmail.tokens.refresh_token) ,
    );

    let resp = client
            .post("https://oauth2.googleapis.com/token")
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(body)
            .send()
            .await?;

    if !resp.status().is_success() {
        let error_text = resp.text().await?;
        println!("Error response: {}", error_text);
        return Err(error_text.into());
    }

    let token_resp: TokenResponse = resp.json().await?;
    Ok(token_resp)
}