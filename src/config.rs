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

// ---------------------------------------------------------------------------
// LLM configuration (loaded from a separate llm_conf.toml)
// ---------------------------------------------------------------------------

/// Top-level wrapper that mirrors the `[llm]` table in llm_conf.toml.
#[derive(Debug, Clone, Deserialize)]
pub struct LlmConfig {
    pub llm: LlmSection,
}

/// Which extraction backend to use.
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum LlmBackend {
    Ollama,
    Cliproxy,
    Remote,
    Heuristics,
}

impl Default for LlmBackend {
    fn default() -> Self {
        Self::Ollama
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct LlmSection {
    #[serde(default)]
    pub backend: LlmBackend,
    #[serde(default)]
    pub ollama: OllamaConfig,
    #[serde(default)]
    pub cliproxy: CliProxyConfig,
    #[serde(default)]
    pub remote: RemoteConfig,
}

impl Default for LlmSection {
    fn default() -> Self {
        Self {
            backend: LlmBackend::Ollama,
            ollama: OllamaConfig::default(),
            cliproxy: CliProxyConfig::default(),
            remote: RemoteConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct OllamaConfig {
    #[serde(default = "default_ollama_url")]
    pub base_url: String,
    #[serde(default = "default_ollama_model")]
    pub model: String,
}

impl Default for OllamaConfig {
    fn default() -> Self {
        Self {
            base_url: default_ollama_url(),
            model: default_ollama_model(),
        }
    }
}

fn default_ollama_url() -> String {
    "http://localhost:11434/v1".to_string()
}

fn default_ollama_model() -> String {
    "qwen3:8b".to_string()
}

#[derive(Debug, Clone, Deserialize)]
pub struct CliProxyConfig {
    #[serde(default = "default_cliproxy_url")]
    pub base_url: String,
    #[serde(default = "default_cliproxy_model")]
    pub model: String,
}

impl Default for CliProxyConfig {
    fn default() -> Self {
        Self {
            base_url: default_cliproxy_url(),
            model: default_cliproxy_model(),
        }
    }
}

fn default_cliproxy_url() -> String {
    "http://127.0.0.1:8317/v1".to_string()
}

fn default_cliproxy_model() -> String {
    "claude-sonnet-4-20250514".to_string()
}

#[derive(Debug, Clone, Deserialize)]
pub struct RemoteConfig {
    #[serde(default = "default_remote_url")]
    pub base_url: String,
    #[serde(default = "default_remote_model")]
    pub model: String,
}

impl Default for RemoteConfig {
    fn default() -> Self {
        Self {
            base_url: default_remote_url(),
            model: default_remote_model(),
        }
    }
}

fn default_remote_url() -> String {
    "https://api.openai.com/v1".to_string()
}

fn default_remote_model() -> String {
    "gpt-4o".to_string()
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

impl LlmConfig {
    /// Load LLM configuration from a TOML file.
    pub fn load(path: impl AsRef<Path>) -> Result<LlmSection, Box<dyn std::error::Error>> {
        let content = fs::read_to_string(&path)?;
        let wrapper: LlmConfig = toml::from_str(&content)?;
        Ok(wrapper.llm)
    }

    /// Resolve the config file path, checking `.config/llm_conf.toml`
    /// relative to the project/working directory.
    pub fn default_path() -> std::path::PathBuf {
        #[cfg(debug_assertions)]
        {
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".config/llm_conf.toml")
        }
        #[cfg(not(debug_assertions))]
        {
            std::path::PathBuf::from(".config/llm_conf.toml")
        }
    }
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
