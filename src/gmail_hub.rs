use google_gmail1::Gmail;
use yup_oauth2::{ApplicationSecret, InstalledFlowAuthenticator, InstalledFlowReturnMethod};

use crate::config::Config;
use crate::simple_refresh::manual_refresh;
use crate::simplestore::SimpleTokenStore;
use hyper_rustls::HttpsConnector;
use hyper_util::client::legacy::connect::HttpConnector;
use std::env;
use std::path::PathBuf;

#[cfg(debug_assertions)]
fn config_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".config")
}

fn config_path() -> PathBuf {
    config_dir().join("oath_cli.toml")
}

pub async fn create_hub()
    -> Result<Gmail<HttpsConnector<HttpConnector>>, Box<dyn std::error::Error>>
{
    let cfg = Config::load(config_path())?;

    let (tok, ttl) = if env::var("REFRESH").is_ok_and(|v| v == "1") {
        println!("Refreshing....");
        let token = manual_refresh(&cfg).await?;
        Config::update_access_token(config_path(), &token.access_token)?;
        (token.access_token, token.expires_in)
    } else {
        (cfg.gmail.tokens.access_token, 3599)
    };

    let secret = ApplicationSecret {
        client_id: cfg.gmail.client_id,
        client_secret: cfg.gmail.client_secret,
        token_uri: cfg.gmail.urls.token_url,
        auth_uri: cfg.gmail.urls.auth_url,
        redirect_uris: vec!["http://localhost".to_string()],
        project_id: None,
        client_email: None,
        auth_provider_x509_cert_url: None,
        client_x509_cert_url: None,
    };

    let auth = InstalledFlowAuthenticator::builder(secret, InstalledFlowReturnMethod::HTTPRedirect)
        .with_storage(Box::new(SimpleTokenStore {
            access_token: tok,
            refresh_token: cfg.gmail.tokens.refresh_token,
            expires_in: ttl,
        }))
        .build()
        .await?;

    let client = hyper_util::client::legacy::Client::builder(hyper_util::rt::TokioExecutor::new())
        .build(
            hyper_rustls::HttpsConnectorBuilder::new()
                .with_webpki_roots()
                .https_or_http()
                .enable_http1()
                .build(),
        );

    Ok(Gmail::new(client, auth))
}
