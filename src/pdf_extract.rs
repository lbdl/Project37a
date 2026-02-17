// src/pdf_extract.rs

use crate::message_db::MessageStore;
use lopdf::Document;
use tracing::{info, warn};

/// Result of attempting to extract text from a PDF.
#[derive(Debug)]
pub enum PdfContent {
    /// The PDF contains extractable text.
    Text(String),
    /// The PDF appears to be scanned / image-only — needs OCR.
    ScannedImage,
    /// Something went wrong during extraction.
    Error(String),
}

/// Minimum number of non-whitespace characters we expect from a
/// "real" text PDF. Below this threshold we treat it as scanned.
const MIN_TEXT_CHARS: usize = 30;

/// Main entry point: takes raw PDF bytes and returns `PdfContent`.
pub fn extract_text_from_pdf(pdf_bytes: &[u8]) -> PdfContent {
    // --- Phase 1: structural check with lopdf ---
    let doc = match Document::load_mem(pdf_bytes) {
        Ok(d) => d,
        Err(e) => return PdfContent::Error(format!("Failed to parse PDF: {e}")),
    };

    if looks_like_scanned(&doc) {
        info!("PDF structural check: likely scanned / image-only");
        return PdfContent::ScannedImage;
    }

    // --- Phase 2: attempt full text extraction ---
    match pdf_extract::extract_text_from_mem(pdf_bytes) {
        Ok(text) => {
            let meaningful: String = text.chars().filter(|c| !c.is_whitespace()).collect();
            if meaningful.len() < MIN_TEXT_CHARS {
                info!(
                    chars = meaningful.len(),
                    "Extracted text too short — treating as scanned"
                );
                PdfContent::ScannedImage
            } else {
                info!(chars = meaningful.len(), "Text extracted successfully");
                PdfContent::Text(text)
            }
        }
        Err(e) => {
            warn!(error = %e, "pdf-extract failed — may be scanned or corrupted");
            PdfContent::ScannedImage
        }
    }
}

/// Heuristic: inspect the PDF object tree for signs that every page
/// is just a single image with no text operators.
///
/// We look at each page's `Resources` dictionary. If a page has
/// XObject images but **no** Font resources, it's almost certainly
/// a scanned page.
fn looks_like_scanned(doc: &Document) -> bool {
    let pages = doc.get_pages();
    if pages.is_empty() {
        return false; // Can't tell — let text extraction try
    }

    let mut image_only_pages = 0;

    for (_page_num, object_id) in &pages {
        let Ok(page_obj) = doc.get_object(*object_id) else {
            continue;
        };
        let Some(page_dict) = page_obj.as_dict().ok() else {
            continue;
        };

        let has_fonts = page_dict
            .get(b"Resources")
            .ok()
            .and_then(|r| doc.dereference(r).ok())
            .and_then(|(_, resolved)| resolved.as_dict().ok())
            .and_then(|res| res.get(b"Font").ok())
            .and_then(|f| doc.dereference(f).ok())
            .and_then(|(_, resolved)| resolved.as_dict().ok())
            .is_some_and(|fonts| !fonts.is_empty());

        let has_images = page_dict
            .get(b"Resources")
            .ok()
            .and_then(|r| doc.dereference(r).ok())
            .and_then(|(_, resolved)| resolved.as_dict().ok())
            .and_then(|res| res.get(b"XObject").ok())
            .and_then(|x| doc.dereference(x).ok())
            .and_then(|(_, resolved)| resolved.as_dict().ok())
            .is_some_and(|xobjs| !xobjs.is_empty());

        if has_images && !has_fonts {
            image_only_pages += 1;
        }
    }

    let total = pages.len();
    let ratio = image_only_pages as f64 / total as f64;
    info!(
        total_pages = total,
        image_only = image_only_pages,
        ratio = format!("{ratio:.2}"),
        "Scanned-page analysis"
    );

    // If ≥80% of pages are image-only, treat the whole PDF as scanned
    ratio >= 0.8
}

/// Open a DB by path and process all unprocessed PDF attachments.
pub fn process_pdfs(db_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    info!(db_path = %db_path, "Opening database for PDF processing");
    let db = MessageStore::new(db_path)?;

    let (total_msgs, processed_msgs, total_pdfs, processed_pdfs) = db.get_counts()?;
    info!(
        messages_total = total_msgs,
        messages_processed = processed_msgs,
        pdfs_total = total_pdfs,
        pdfs_processed = processed_pdfs,
        "Database statistics"
    );

    run_pdf_extraction(&db)?;
    Ok(())
}

/// Iterate over unprocessed attachments, classify them, and persist results.
pub fn run_pdf_extraction(db: &MessageStore) -> Result<(), Box<dyn std::error::Error>> {
    let unprocessed = db.get_unprocessed_attachments()?;
    info!(
        count = unprocessed.len(),
        "Unprocessed attachments to extract"
    );

    for att in &unprocessed {
        let att_id = att.id.expect("attachment must have an id from DB");
        let span = tracing::info_span!("pdf", filename = %att.filename);
        let _guard = span.enter();

        match extract_text_from_pdf(&att.pdf_data) {
            PdfContent::Text(text) => {
                info!(chars = text.len(), "Extracted text from PDF");
                db.set_attachment_extraction(att_id, "text", Some(&text))?;
            }
            PdfContent::ScannedImage => {
                info!("PDF is scanned — needs OCR / vision model");
                db.set_attachment_extraction(att_id, "scanned", None)?;
            }
            PdfContent::Error(e) => {
                tracing::error!(error = %e, "Failed to process PDF");
                db.set_attachment_extraction(att_id, "error", Some(&e))?;
            }
        }
    }

    // Summary
    let text_count = db.get_text_attachments()?.len();
    let scanned_count = db.get_scanned_attachments()?.len();
    info!(
        text = text_count,
        scanned = scanned_count,
        "Extraction complete — ready for heuristics / OCR"
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_garbage_bytes() {
        let result = extract_text_from_pdf(b"this is not a pdf");
        assert!(matches!(result, PdfContent::Error(_)));
    }
}
