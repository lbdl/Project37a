use serde::Deserialize;
use std::{fs, path::Path};
use toml_edit::{DocumentMut, value};

#[derive(Deserialize)]
pub struct Config {
    #[serde(rename = "gmail_oauth")]
    pub gmail: GmailConfig,
    #[serde(default = "default_db_path")]
    pub db_path: String,
}

fn default_db_path() -> String {
    "msgstore/messages.db".to_string()
}

#[derive(Deserialize)]
pub struct GmailConfig {
    pub client_id: String,
    pub client_secret: String,
    pub tokens: Tokens,
    #[serde(rename = "cmd_urls")]
    pub cmds: CmdStrings,
    pub urls: AuthUrls,
    pub grants: Grant,
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

#[derive(Deserialize)]
pub struct Grant {
    pub refresh: String,
}

impl Config {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, Box<dyn std::error::Error>> {
        let content = fs::read_to_string(path)?;
        Ok(toml::from_str(&content)?)
    }

    pub fn update_access_token(
        path: impl AsRef<Path>,
        new_token: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let content = fs::read_to_string(&path)?;
        let mut doc = content.parse::<DocumentMut>()?;

        doc["gmail_oauth"]["tokens"]["access_token"] = value(new_token);

        fs::write(&path, doc.to_string())?;
        Ok(())
    }
}
