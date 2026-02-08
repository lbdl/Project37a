use serde::Deserialize;
use std::{fs, path::Path};

#[derive(Deserialize)]
pub struct Config {
    #[serde(rename = "gmail_oauth")]
    pub gmail: GmailConfig,
}

#[derive(Deserialize)]
pub struct GmailConfig {
    pub client_id: String,
    pub client_secret: String,
    pub tokens: Tokens,
    #[serde(rename = "cmd_urls")]
    pub cmds: CmdStrings,
    pub urls: AuthUrls,
}

#[derive(Deserialize)]
pub struct Tokens {
    pub refresh_token: String,
    pub access_token: String,
    #[serde(rename = "authinitial")]
    pub auth_initial: String,
}

#[derive(Deserialize)]
pub struct CmdStrings {
    pub initial_auth: String,
    pub refresh_curl: String,
}

#[derive(Deserialize)]
pub struct AuthUrls {
    pub token_url: String,
    pub auth_url: String,
}

#[derive(Deserialize)]
pub struct Metadata {
    pub scopes: Vec<String>,
}

impl Config {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, Box<dyn std::error::Error>> {
        let content = fs::read_to_string(path)?;
        Ok(toml::from_str(&content)?)
    }
}