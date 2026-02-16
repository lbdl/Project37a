mod config;
mod filter;
mod gmail_hub;
mod message_db;
mod message_processor;
mod simple_refresh;
mod simplestore;

use message_db::{MessageStore, StoredAttachment, StoredMessage};
use tracing::info;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // init tracing
    tracing_subscriber::fmt()
        .with_target(true)
        .with_level(true)
        .with_env_filter("info")
        .init();

    // Install crypto provider
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

    let hub = gmail_hub::create_hub().await?;
    let cfg = config::Config::load(".config/oath_cli.toml")?;
    let db = MessageStore::new(&cfg.db_path)?;

    let user = "mmsoft.mudit@gmail.com";
    let maxsoft = "from:*@maxsoft.sg AND after:2025/01/01 AND filename:pdf";
    let fedex = "from:thicc@fedex.com AND after:2025/01/01";

    let maxsoft_msgs = filter::get_message_ids(&hub, maxsoft, user).await?;
    let fedex_msgs = filter::get_message_ids(&hub, fedex, user).await?;

    for msg in filter::fetch_msgs(&hub, &user, maxsoft_msgs).await? {
        let message_id = msg.message_id.as_ref().unwrap();
        let unknown = String::from("unknown");
        let date = msg.date.as_ref().unwrap_or(&unknown);

        // Generate UID and store in database
        let uid = MessageStore::generate_uid(message_id, date, user);

        let stored_msg = StoredMessage {
            uid: uid.clone(),
            message_id: message_id.clone(),
            user: user.to_string(),
            date: date.clone(),
            from_addr: msg.from_addr.clone(),
            subject: msg.subject.clone(),
            plain_text: msg.plain.clone(),
            html: msg.html.clone(),
            has_attachments: !msg.attachments.is_empty(),
            is_processed: false,
        };

        db.upsert_message(&stored_msg)?;

        // Store PDF attachments
        for attachment in &msg.attachments {
            if let Some(pdf_data) = &attachment.data {
                let stored_attachment = StoredAttachment {
                    id: None,
                    message_uid: uid.clone(),
                    filename: attachment.filename.clone(),
                    attachment_id: attachment.attachment_id.clone(),
                    pdf_data: pdf_data.clone(),
                    is_processed: false,
                };
                db.insert_attachment(&stored_attachment)?;
            }
        }

        info!(uid = %uid, id = %message_id, attachments = msg.attachments.len(), "STORED");
    }

    // Print statistics
    let (total_msgs, processed_msgs, total_pdfs, processed_pdfs) = db.get_counts()?;
    info!(
        messages_total = total_msgs,
        messages_processed = processed_msgs,
        pdfs_total = total_pdfs,
        pdfs_processed = processed_pdfs,
        "Database statistics"
    );

    Ok(())
}
