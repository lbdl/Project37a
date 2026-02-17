use crate::message_db::{MessageStore, StoredAttachment, StoredMessage};
use crate::message_processor as mproc;
use crate::message_processor::EmailData;
use google_gmail1::api::{MessagePart, MessagePartHeader, Scope};
use hyper_rustls::HttpsConnector;
use hyper_util::client::legacy::connect::HttpConnector;
use tracing::{info, info_span};

pub async fn fetch_msgs(
    hub: &google_gmail1::Gmail<HttpsConnector<HttpConnector>>,
    user: &str,
    ids: Vec<String>,
) -> Result<Vec<EmailData>, Box<dyn std::error::Error>> {
    let mut emails = Vec::new();

    for id in ids {
        info!(user = %user, id = %id, "Starting email fetch");
        let (_, email) = hub
            .users()
            .messages_get(user, &id)
            .add_scope(Scope::Readonly)
            .doit()
            .await?;

        info!(mail = ?email.id, "Fetched mail id:");

        let payload = email.payload.as_ref().unwrap();

        let headers = mproc::get_headers(
            payload.headers.as_ref(),
            vec!["From", "Subject", "To", "Date"],
        );

        info!(
            from = headers.get(0).unwrap_or(&""),
            // subj = headers.get(1).unwrap_or(&""),
            // to = headers.get(2).unwrap_or(&""),
            date = headers.get(3).unwrap_or(&""),
            "MAIL: "
        );

        let mail_data =
            mproc::get_email_data(email.payload.as_ref(), id.clone(), payload.headers.as_ref());

        // Fetch actual PDF data for attachments that only have an attachment_id
        let mut mail_data = mail_data;
        for attachment in &mut mail_data.attachments {
            if attachment.data.is_none() {
                if let Some(att_id) = &attachment.attachment_id {
                    info!(filename = %attachment.filename, "Fetching attachment data");
                    let (_, att) = hub
                        .users()
                        .messages_attachments_get(user, &id, att_id)
                        .add_scope(Scope::Readonly)
                        .doit()
                        .await?;
                    attachment.data = att.data;
                }
            }
        }

        emails.push(mail_data);
    }
    Ok(emails)
}

/// Fetch messages by IDs, store them (with PDF attachments) in the database, and return the count stored.
pub async fn fetch_and_store(
    hub: &google_gmail1::Gmail<HttpsConnector<HttpConnector>>,
    user: &str,
    ids: Vec<String>,
    db: &MessageStore,
) -> Result<usize, Box<dyn std::error::Error>> {
    let msgs = fetch_msgs(hub, user, ids).await?;
    let mut count = 0;

    for msg in &msgs {
        let message_id = msg.message_id.as_ref().unwrap();
        let unknown = String::from("unknown");
        let date = msg.date.as_ref().unwrap_or(&unknown);

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

        for attachment in &msg.attachments {
            if let Some(pdf_data) = &attachment.data {
                info!(message_id = ?message_id, attachment_id = ?attachment.attachment_id, "STORING ATTACHMENT");
                let stored_attachment = StoredAttachment {
                    id: None,
                    message_uid: uid.clone(),
                    filename: attachment.filename.clone(),
                    attachment_id: attachment.attachment_id.clone(),
                    pdf_data: pdf_data.clone(),
                    is_processed: false,
                    content_type: None,
                    extracted_text: None,
                };
                db.insert_attachment(&stored_attachment)?;
            }
        }

        info!(uid = %uid, id = %message_id, attachments = msg.attachments.len(), "STORED");
        count += 1;
    }

    Ok(count)
}

pub async fn get_message_ids(
    hub: &google_gmail1::Gmail<HttpsConnector<HttpConnector>>,
    query: &str,
    user: &str,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    get_message_ids_recursive(hub, query, None, user).await
}

fn get_message_ids_recursive<'a>(
    hub: &'a google_gmail1::Gmail<HttpsConnector<HttpConnector>>,
    query: &'a str,
    page_token: Option<&'a str>,
    user: &'a str,
) -> std::pin::Pin<
    Box<dyn std::future::Future<Output = Result<Vec<String>, Box<dyn std::error::Error>>> + 'a>,
> {
    info!(user = %user, query = %query, has_page_token = page_token.is_some(), "Starting id fetch");

    Box::pin(async move {
        let mut req = hub.users().messages_list(user).q(query);

        if let Some(token) = page_token {
            req = req.page_token(token);
        }

        let (_, response) = req.doit().await?;

        let mut ids: Vec<String> = response
            .messages
            .unwrap_or_default()
            .into_iter()
            .filter_map(|m| m.id)
            .collect();

        if let Some(token) = response.next_page_token {
            info!(next_token = %token, "Fetching next page");
            let mut next_ids = get_message_ids_recursive(hub, query, Some(&token), user).await?;
            ids.append(&mut next_ids);
        }
        info!(matches = ids.len(), "Page complete");
        Ok(ids)
    })
}
