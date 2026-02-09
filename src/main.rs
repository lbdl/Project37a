mod config;
mod filter;
mod simplestore;
mod simple_refresh;

use google_gmail1::{api::Scope, Gmail};
use yup_oauth2::{
    storage::TokenStorage,
    ApplicationSecret,
    InstalledFlowAuthenticator,
    InstalledFlowReturnMethod,
};

use tracing_subscriber;
use std::env;
use std::path::PathBuf;
use hyper_rustls::HttpsConnector;
use hyper_util::client::legacy::connect::HttpConnector;
use yup_oauth2::authenticator::Authenticator;
use simplestore::SimpleTokenStore;
use config::Config;
use simple_refresh::manual_refresh;

#[cfg(debug_assertions)]
fn config_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".config")
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cfg = Config::load(config_dir().join("oath_cli.toml"))?;
    let auth: Authenticator<HttpsConnector<HttpConnector>>;
    let tok:String;
    let ttl:i64;

    //init tracing
    tracing_subscriber::fmt()
        .with_target(true)
        .with_level(true)
        .with_env_filter("info")  // or use RUST_LOG env var
        .init();

    // Install crypto provider
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

    let user = "mmsoft.mudit@gmail.com";
    let maxsoft = "from:*@maxsoft.sg AND after:2025/01/01 AND filename:pdf";
    let fedex = "from:thicc@fedex.com AND after:2025/01/01";



    // handle manual refreshing we dont really need it but lets be complete
    if env::var("REFRESH").is_ok_and(|v| v == "1") {
        // Force token fetch/refresh
        println!("Refreshing....");
        let _token = manual_refresh(&cfg).await?;
        tok = _token.access_token;
        ttl = _token.expires_in;
    } else {
        tok = cfg.gmail.tokens.access_token;
        ttl= 3599;
    }

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

    // Create authenticator with your existing tokens
    auth = InstalledFlowAuthenticator::builder(
        secret,
        InstalledFlowReturnMethod::HTTPRedirect
    )
        .with_storage(Box::new(SimpleTokenStore {
            access_token: tok,
            refresh_token: cfg.gmail.tokens.refresh_token,
            expires_in: ttl,
        }))
        .build()
        .await?;

    let client = hyper_util::client::legacy::Client::builder(
        hyper_util::rt::TokioExecutor::new()
    ).build(
        hyper_rustls::HttpsConnectorBuilder::new()
            .with_webpki_roots()
            .https_or_http()
            .enable_http1()
            .build()
    );

    let hub = Gmail::new(client, auth);

    let maxsoft_msgs = filter::get_message_ids(&hub, maxsoft, user).await?;
    let fedex_msgs = filter::get_message_ids(&hub, fedex, user).await?;


    // TODO refactor the below to use the prefetch m ids from the filter mod
    let (_, msgs) = hub.users().messages_list(user)
        .q(maxsoft)
        .max_results(100)
        .doit()
        .await?;

    if let Some(messages) = msgs.messages.as_ref() {

        println!("MSG_ESTIMATE: {:?}", msgs.result_size_estimate);

        for m in messages {
            let m_id = m.id.clone().unwrap();
            println!("--->FETCH ID: {}", m.id.clone().unwrap());
            let (_, email) = hub.users()
                .messages_get(user, &m_id)
                .add_scope(Scope::Readonly)
                .doit()
                .await?;

            if let Some(payload) = &email.payload {
                if let Some(headers) = &payload.headers {
                    for h in headers {
                        if h.name.as_deref() == Some("From") {
                            println!("---->FROM: {}", h.value.clone().unwrap_or_default());
                        }
                        if h.name.as_deref() == Some("Date") {
                            println!("---->DATE: {}", h.value.clone().unwrap_or_default());
                        }
                    }
                }
            }
        }
    }
    //TODO given a vec<msg> store this somewhere for analysis

    Ok(())
}
