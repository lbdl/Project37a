use hyper_rustls::HttpsConnector;
use hyper_util::client::legacy::connect::HttpConnector;
use tracing::{info, info_span};
use google_gmail1::{api::Scope};
use google_gmail1::api::MessagePartHeader;

pub async fn fetch_msgs(
    hub: &google_gmail1::Gmail<HttpsConnector<HttpConnector>>,
                        user: &str,
                        ids: Vec<String>,
) -> Result<i64, Box<dyn std::error::Error>> {
    for id in ids {
        info!(user = %user, id = %id, "Starting email fetch");
        let (_, email) = hub.users()
            .messages_get(user, &id)
            .add_scope(Scope::Readonly)
            .doit()
            .await?;

        info!(mail = ?email.id, "Fetched mail id:");

        let payload = email.payload.as_ref().unwrap();

        let from = get_headers(payload.headers.as_ref(), "From");
        info!(from = ?from, "mail From:");
        // if let Some(msg) = &email{
        //     info!(mail = ?email, "Fetched mail -> email:");
        // }
    }
    // return something
    Ok((1))
}

fn get_headers<'a>(headers: Option<&'a Vec<MessagePartHeader>>, name: &str) -> Option<&'a str> {
    headers?.iter()
        .find(|h| h.name.as_deref() == Some(name))
        .and_then(|h| h.value.as_deref())
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
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<String>, Box<dyn std::error::Error>>> + 'a>> {

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