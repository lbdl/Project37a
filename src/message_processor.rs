use google_gmail1::api::{MessagePart, MessagePartHeader};
use tracing::info;

#[derive(Default)]
pub struct EmailData {
    pub message_id: Option<String>,
    pub plain: Option<String>,
    pub html: Option<String>,
    pub attachments: Vec<Attachment>,
}

pub struct Attachment {
    pub filename: String,
    pub attachment_id: Option<String>, // For Gmail API fetch
    pub data: Option<Vec<u8>>,         // Inline data if available
}

pub fn get_email_data<'a>(msg: Option<&'a MessagePart>, message_id: String) -> EmailData {
    //check msg type are we MIME or simple message
    //we almost certainly never will be simple because we are gmail
    let part = msg.unwrap();
    let mut data = EmailData::default();
    data.message_id = Some(message_id);
    info!(mime = part.mime_type, "MIME:");
    recurse_over_body(part, &mut data);
    data
}

fn recurse_over_body<'a>(part: &MessagePart, content: &mut EmailData) {
    match part.mime_type.as_deref() {
        Some("text/plain") => {
            // info!("PROC: plain");
            if let Some(data) = part.body.as_ref().and_then(|b| b.data.as_ref()) {
                content.plain = String::from_utf8(data.clone()).ok();
            }
        }
        Some("text/html") => {
            // info!("PROC: html");
            if let Some(data) = part.body.as_ref().and_then(|b| b.data.as_ref()) {
                content.html = String::from_utf8(data.clone()).ok();
            }
        }
        Some("application/pdf") => {
            // info!("PROC: pdf");
            if let Some(filename) = &part.filename {
                let attachment = Attachment {
                    filename: filename.clone(),
                    attachment_id: part.body.as_ref().and_then(|b| b.attachment_id.clone()),
                    data: part.body.as_ref().and_then(|b| b.data.clone()),
                };
                content.attachments.push(attachment);
            }
        }
        Some(mime) if mime.starts_with("multipart/") => {
            // info!("PROC: multi");
            if let Some(parts) = &part.parts {
                for sub_part in parts {
                    recurse_over_body(sub_part, content);
                }
            }
        }
        _ => {}
    }
}

pub fn get_headers<'a>(
    headers: Option<&'a Vec<MessagePartHeader>>,
    names: Vec<&str>,
) -> Vec<&'a str> {
    names
        .into_iter()
        .filter_map(|n| get_header(headers, n))
        .collect()
}

fn get_header<'a>(headers: Option<&'a Vec<MessagePartHeader>>, name: &str) -> Option<&'a str> {
    headers?
        .iter()
        .find(|h| h.name.as_deref() == Some(name))
        .and_then(|h| h.value.as_deref())
}
