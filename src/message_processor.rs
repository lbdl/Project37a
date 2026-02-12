use google_gmail1::api::{MessagePart, MessagePartHeader};
use tracing::info;

#[derive(Default)]
struct EmailData {
    plain: Option<String>,
    hmtl: Option<String>,
    attachments: Vec<Attachment>,
}

struct Attachment {
    filename: String,
    attachment_id: Option<String>, // For Gmail API fetch
    data: Option<Vec<u8>>,         // Inline data if available
}

pub fn get_email_data<'a>(msg: Option<&'a MessagePart>) -> EmailData {
    //check msg type are we MIME or simplr message
    //we almost certainly never will be simple because we are gmail
    let part = msg.unwrap();
    info!(mime = part.mime_type, "MIME:");
    let mut msg_data = EmailData::default();
    msg_data
}

fn recurse_over_body<'a>(part: &MessagePart) -> EmailData {
    match part.mime_type.as_deref() {
        Some("text/plain") => { EmailData::default()}
        Some("text/html") => {EmailData::default()}
        Some("application/pdf") => {EmailData::default()}
        Some(mime) if mime.starts_with("multipart/") => {EmailData::default()}
        _ => {EmailData::default()}
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
