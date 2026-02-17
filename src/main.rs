mod config;
mod filter;
mod gmail_hub;
mod heuristics;
mod llm_extract;
mod message_db;
mod message_processor;
mod pdf_extract;
mod simple_refresh;
mod simplestore;

use crate::config::{LlmConfig, LlmSection};
use message_db::MessageStore;
use tracing::info;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // init tracing
    tracing_subscriber::fmt()
        .with_target(true)
        .with_level(true)
        .with_env_filter("info")
        .init();

    let args: Vec<String> = std::env::args().collect();

    // Dispatch subcommands
    if args.len() >= 2 && args[1] == "process-pdfs" {
        let db_path = args
            .get(2)
            .map(|s| s.as_str())
            .unwrap_or("msgstore/messages.db");

        let llm_config = load_llm_config();
        return pdf_extract::process_pdfs(db_path, &llm_config).await;
    }

    // cargo run -- test-pdf <attachment_id> [db_path]
    if args.len() >= 3 && args[1] == "test-pdf" {
        let att_id: i64 = args[2]
            .parse()
            .map_err(|_| format!("Invalid attachment ID: {}", args[2]))?;
        let db_path = args
            .get(3)
            .map(|s| s.as_str())
            .unwrap_or("msgstore/messages.db");

        let llm_config = load_llm_config();
        return pdf_extract::test_single_pdf(db_path, att_id, &llm_config).await;
    }

    // --- Default: full Gmail fetch + process flow ---
    // Install crypto provider
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

    let hub = gmail_hub::create_hub().await?;
    let cfg = config::Config::load(".config/oath_cli.toml")?;
    let db = MessageStore::new(&cfg.db_path)?;

    let user = "mmsoft.mudit@gmail.com";
    let maxsoft = "from:*@maxsoft.sg AND after:2025/11/01 AND filename:pdf";
    let fedex = "from:thicc@fedex.com AND after:2025/01/01";

    let maxsoft_msgs = filter::get_message_ids(&hub, maxsoft, user).await?;
    let fedex_msgs = filter::get_message_ids(&hub, fedex, user).await?;

    filter::fetch_and_store(&hub, user, maxsoft_msgs, &db).await?;

    // Print statistics
    let (total_msgs, processed_msgs, total_pdfs, processed_pdfs) = db.get_counts()?;
    info!(
        messages_total = total_msgs,
        messages_processed = processed_msgs,
        pdfs_total = total_pdfs,
        pdfs_processed = processed_pdfs,
        "Database statistics"
    );

    // Process unprocessed PDF attachments
    pdf_extract::run_pdf_extraction(&db)?;

    Ok(())
}

/// Load LLM config from `.config/llm_conf.toml`, falling back to defaults.
fn load_llm_config() -> LlmSection {
    let path = LlmConfig::default_path();
    match LlmConfig::load(&path) {
        Ok(section) => {
            info!(path = %path.display(), "Loaded LLM config");
            section
        }
        Err(e) => {
            info!(
                error = %e,
                "Could not load LLM config — using default (Ollama)"
            );
            LlmSection::default()
        }
    }
}
