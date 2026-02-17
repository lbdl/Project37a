// src/llm_extract.rs

use crate::config::{LlmBackend, LlmSection};
use crate::heuristics::InvoiceData;
use crate::message_db::MessageStore;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

/// The prompt template that instructs the model to extract structured invoice data.
const SYSTEM_PROMPT: &str = r#"You are an invoice data extraction assistant.
Given raw text extracted from a PDF invoice, extract structured data and return ONLY valid JSON.

The JSON must match this schema exactly:
{
  "vendor": "string or null",
  "buyer": "string or null",
  "invoice_no": "string or null",
  "invoice_date": "string or null",
  "currency": "string or null (e.g. USD, SGD)",
  "total_amount": number or null,
  "total_pieces": integer or null,
  "ship_from": "string or null",
  "ship_to": "string or null",
  "shipping_method": "string or null",
  "line_items": [
    {
      "description": "string",
      "qty": integer,
      "unit_price": number,
      "amount": number
    }
  ],
  "packing_items": [
    {
      "carton": "string",
      "description": "string",
      "ctns": integer,
      "qty": integer,
      "net_wt_per_ctn": number,
      "gross_wt_per_ctn": number,
      "measurement": "string"
    }
  ],
  "packing_totals": {
    "total_cartons": integer,
    "total_qty": integer,
    "total_net_wt": number,
    "total_gross_wt": number
  } or null
}

Notes:
- The text may be garbled due to PDF column extraction issues. Do your best to reconstruct the data.
- Use null for fields you cannot determine.
- Return ONLY the JSON object, no markdown fences, no commentary."#;

#[derive(Debug, Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    temperature: f64,
}

#[derive(Debug, Serialize, Deserialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatMessage,
}

/// Resolved endpoint configuration ready to make API calls.
struct ResolvedEndpoint {
    base_url: String,
    model: String,
    api_key: String,
}

/// Resolve the LLM config section into a concrete endpoint.
fn resolve_endpoint(llm: &LlmSection) -> Result<ResolvedEndpoint, Box<dyn std::error::Error>> {
    match llm.backend {
        LlmBackend::Ollama => {
            info!(
                url = %llm.ollama.base_url,
                model = %llm.ollama.model,
                "Using Ollama (local) backend"
            );
            Ok(ResolvedEndpoint {
                base_url: llm.ollama.base_url.clone(),
                model: llm.ollama.model.clone(),
                api_key: "ollama".to_string(), // required by API but ignored
            })
        }
        LlmBackend::Cliproxy => {
            info!(
                url = %llm.cliproxy.base_url,
                model = %llm.cliproxy.model,
                "Using CLIProxyAPI backend"
            );
            Ok(ResolvedEndpoint {
                base_url: llm.cliproxy.base_url.clone(),
                model: llm.cliproxy.model.clone(),
                api_key: "cliproxy".to_string(), // CLIProxyAPI uses OAuth, not API keys
            })
        }
        LlmBackend::Remote => {
            let api_key = std::env::var("LLM_API_KEY")
                .map_err(|_| "LLM_API_KEY env var required for remote backend")?;
            info!(
                url = %llm.remote.base_url,
                model = %llm.remote.model,
                "Using remote API backend"
            );
            Ok(ResolvedEndpoint {
                base_url: llm.remote.base_url.clone(),
                model: llm.remote.model.clone(),
                api_key,
            })
        }
        LlmBackend::Heuristics => {
            Err("Heuristics backend selected — LLM extraction not needed".into())
        }
    }
}

/// Check if the Ollama server is reachable.
async fn check_ollama_health(client: &Client, base_url: &str) -> bool {
    // Ollama's health endpoint is at the root (not under /v1)
    let health_url = base_url.trim_end_matches("/v1").trim_end_matches("/v1/");

    match client
        .get(health_url)
        .timeout(std::time::Duration::from_secs(3))
        .send()
        .await
    {
        Ok(resp) => {
            if resp.status().is_success() {
                info!("Ollama server is reachable");
                true
            } else {
                warn!(status = %resp.status(), "Ollama server returned non-OK status");
                false
            }
        }
        Err(e) => {
            warn!(error = %e, "Ollama server not reachable");
            false
        }
    }
}

/// Send extracted text to an LLM and parse the structured invoice data.
async fn extract_invoice_with_llm(
    client: &Client,
    endpoint: &ResolvedEndpoint,
    extracted_text: &str,
) -> Result<InvoiceData, Box<dyn std::error::Error>> {
    // Truncate very long texts to stay within context limits
    let max_chars = 12_000;
    let text = if extracted_text.len() > max_chars {
        &extracted_text[..max_chars]
    } else {
        extracted_text
    };

    let request = ChatRequest {
        model: endpoint.model.clone(),
        messages: vec![
            ChatMessage {
                role: "system".to_string(),
                content: SYSTEM_PROMPT.to_string(),
            },
            ChatMessage {
                role: "user".to_string(),
                content: format!("Extract invoice data from the following PDF text:\n\n{text}"),
            },
        ],
        temperature: 0.0,
    };

    let url = format!("{}/chat/completions", endpoint.base_url);

    let response = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", endpoint.api_key))
        .json(&request)
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("LLM API error {status}: {body}").into());
    }

    let chat_response: ChatResponse = response.json().await?;
    let content = chat_response
        .choices
        .first()
        .map(|c| c.message.content.as_str())
        .ok_or("Empty response from LLM")?;

    // Strip markdown fences if the model added them despite instructions
    let json_str = content
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();

    // Some models (especially with /think mode) may prepend reasoning text.
    // Find the first '{' and last '}' to extract just the JSON object.
    let json_str = extract_json_object(json_str)?;

    let invoice: InvoiceData = serde_json::from_str(json_str).map_err(|e| {
        format!("Failed to parse LLM response as InvoiceData: {e}\nRaw: {json_str}")
    })?;

    Ok(invoice)
}

/// Extract the outermost JSON object from a string that may contain
/// surrounding text (e.g. thinking tokens from qwen3).
fn extract_json_object(s: &str) -> Result<&str, Box<dyn std::error::Error>> {
    let start = s.find('{').ok_or("No '{' found in LLM response")?;
    let end = s.rfind('}').ok_or("No '}' found in LLM response")?;
    if end <= start {
        return Err("Malformed JSON in LLM response".into());
    }
    Ok(&s[start..=end])
}

/// Extract invoice data from a single text string (for testing).
pub async fn run_llm_extraction_single(
    text: &str,
    llm_config: &LlmSection,
) -> Result<InvoiceData, Box<dyn std::error::Error>> {
    let endpoint = resolve_endpoint(llm_config)?;
    let client = Client::new();

    if llm_config.backend == LlmBackend::Ollama {
        if !check_ollama_health(&client, &endpoint.base_url).await {
            return Err(format!(
                "Ollama is not running at {}. Start it with: ollama serve",
                endpoint.base_url
            )
            .into());
        }
    }

    extract_invoice_with_llm(&client, &endpoint, text).await
}

/// Run LLM-based extraction on all text-classified attachments.
pub async fn run_llm_extraction(
    db: &MessageStore,
    llm_config: &LlmSection,
) -> Result<(), Box<dyn std::error::Error>> {
    let endpoint = resolve_endpoint(llm_config)?;
    let client = Client::new();

    // Health check for local backends
    if llm_config.backend == LlmBackend::Ollama {
        if !check_ollama_health(&client, &endpoint.base_url).await {
            return Err(format!(
                "Ollama is not running at {}. Start it with: ollama serve",
                endpoint.base_url
            )
            .into());
        }
    }

    let text_attachments = db.get_text_attachments()?;
    info!(
        count = text_attachments.len(),
        backend = ?llm_config.backend,
        model = %endpoint.model,
        "Text attachments for LLM extraction"
    );

    for att in &text_attachments {
        let att_id = att.id.expect("attachment must have an id from DB");
        let span = tracing::info_span!("llm_extract", id = att_id, filename = %att.filename);
        let _guard = span.enter();

        let Some(ref text) = att.extracted_text else {
            warn!("No extracted text despite content_type = text");
            continue;
        };

        match extract_invoice_with_llm(&client, &endpoint, text).await {
            Ok(invoice) => {
                let (filled, total) = invoice.coverage();
                info!(
                    filled, total,
                    invoice_no = ?invoice.invoice_no,
                    vendor = ?invoice.vendor,
                    total_amount = ?invoice.total_amount,
                    line_items = invoice.line_items.len(),
                    "LLM extraction result"
                );

                let json = serde_json::to_string_pretty(&invoice)?;
                info!(json_len = json.len(), "Storing structured invoice JSON");
                // TODO: persist `json` to a new DB column or table
            }
            Err(e) => {
                tracing::error!(error = %e, "LLM extraction failed for attachment {att_id}");
            }
        }
    }

    Ok(())
}
