use async_trait::async_trait;
use yup_oauth2::storage::{TokenInfo, TokenStorage};
use yup_oauth2::error::TokenStorageError;
use time::{Duration, OffsetDateTime};

pub struct SimpleTokenStore {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: i64,
}

#[async_trait]
impl TokenStorage for SimpleTokenStore {
    async fn set(&self, _scopes: &[&str], _token: TokenInfo) -> Result<(), TokenStorageError> {
        // In production, persist updated tokens here
        Ok(())
    }

    async fn get(&self, _scopes: &[&str]) -> Option<TokenInfo> {
        Some(TokenInfo {
            access_token: Some(self.access_token.clone()),
            refresh_token: Some(self.refresh_token.clone()),
            expires_at: Some(OffsetDateTime::now_utc() + Duration::seconds(self.expires_in)),
            id_token: None,
        })
    }
}