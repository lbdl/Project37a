use super::{InvoiceData, LineItem, PackingItem, PackingTotals};
use regex::Regex;

/// Main extraction entry point — uses keyword-anchored regex patterns.
pub fn extract(text: &str) -> InvoiceData {
    InvoiceData {
        vendor: extract_vendor(text),
        buyer: extract_buyer(text),
        invoice_no: extract_invoice_no(text),
        invoice_date: extract_invoice_date(text),
        currency: extract_currency(text),
        total_amount: extract_total_amount(text),
        total_pieces: extract_total_pieces(text),
        ship_from: extract_ship_from(text),
        ship_to: extract_ship_to(text),
        shipping_method: extract_shipping_method(text),
        line_items: extract_line_items(text),
        packing_items: extract_packing_items(text),
        packing_totals: extract_packing_totals(text),
    }
}

// ---------------------------------------------------------------------------
// Scalar field extractors
// ---------------------------------------------------------------------------

fn extract_invoice_no(text: &str) -> Option<String> {
    // Matches "Invoice No." or "Invoice No" followed by optional punctuation then the value
    let re = Regex::new(r"(?i)Invoice\s+No\.?\s*:?\s*([A-Za-z0-9\-/]+)").ok()?;
    re.captures(text).map(|c| c[1].trim().to_string())
}

fn extract_invoice_date(text: &str) -> Option<String> {
    // "Invoice Date" followed by a date like "February 16, 2026" or "16/02/2026"
    let re = Regex::new(
        r"(?i)Invoice\s+Date\s*:?\s*([A-Za-z]+\s+\d{1,2},?\s+\d{4}|\d{1,2}[/\-]\d{1,2}[/\-]\d{2,4})",
    )
    .ok()?;
    re.captures(text).map(|c| c[1].trim().to_string())
}

fn extract_currency(text: &str) -> Option<String> {
    // Look for US$, USD, SGD, EUR, etc.
    let re = Regex::new(r"(?i)\b(US\$|USD|SGD|EUR|GBP|THB|JPY)\b").ok()?;
    let cap = re.captures(text)?;
    let raw = cap[1].to_uppercase();
    // Normalise "US$" → "USD"
    Some(if raw == "US$" { "USD".to_string() } else { raw })
}

fn extract_total_amount(text: &str) -> Option<f64> {
    // Look for "TOTAL" followed by a number (the invoice grand total).
    // We want the TOTAL that sits near the line items, not packing totals.
    // Strategy: find all "TOTAL" + number pairs, take the one before "PACKING LIST".
    let packing_pos = text
        .to_uppercase()
        .find("PACKING LIST")
        .unwrap_or(text.len());
    let invoice_section = &text[..packing_pos];

    let re = Regex::new(r"(?i)TOTAL\s+(\d[\d,]*\.?\d*)").ok()?;
    // Take the last TOTAL match in the invoice section (skips sub-totals)
    let mut last: Option<f64> = None;
    for cap in re.captures_iter(invoice_section) {
        if let Ok(v) = cap[1].replace(',', "").parse::<f64>() {
            last = Some(v);
        }
    }
    last
}

fn extract_total_pieces(text: &str) -> Option<u32> {
    let re = Regex::new(r"(?i)TOTAL\s+PCS\s+(\d+)").ok()?;
    re.captures(text).and_then(|c| c[1].parse::<u32>().ok())
}

fn extract_vendor(text: &str) -> Option<String> {
    // The vendor/shipper is typically the company with the address block
    // that appears after "Shipped per" or is the sender (Singapore side).
    // In Soft Source invoices: "SOFT SOURCE PTE LTD" appears as the shipper.
    // Generic: look for "PTE LTD" or "PTE. LTD" company near the top.
    // We also check for a block right after "For Account & risk of Messers"
    // — the company AFTER that is the buyer, so the OTHER company is the vendor.

    // Heuristic: if we find the buyer, the other company name is the vendor.
    let companies = extract_company_names(text);
    let buyer = extract_buyer(text);

    // Return the first company that isn't the buyer
    for company in &companies {
        if let Some(ref b) = buyer {
            if !company.to_uppercase().contains(&b.to_uppercase()) {
                return Some(company.clone());
            }
        }
    }

    companies.into_iter().next()
}

fn extract_buyer(text: &str) -> Option<String> {
    // "For Account & risk of Messers" is followed by the buyer name
    let re = Regex::new(r"(?i)(?:For\s+)?Account\s*&?\s*risk\s+of\s+Messers?\s*\n\s*(.+)").ok()?;
    re.captures(text).map(|c| c[1].trim().to_string())
}

fn extract_ship_from(text: &str) -> Option<String> {
    let re = Regex::new(r"(?i)From\s*:\s*([A-Za-z\s]+?)(?:\s{2,}|To\s*:|\n)").ok()?;
    re.captures(text).map(|c| c[1].trim().to_uppercase())
}

fn extract_ship_to(text: &str) -> Option<String> {
    let re = Regex::new(r"(?i)To\s*:\s*([A-Za-z\s]+?)(?:\s{2,}|\n)").ok()?;
    re.captures(text).map(|c| c[1].trim().to_uppercase())
}

fn extract_shipping_method(text: &str) -> Option<String> {
    // "Shipped per :" followed by the carrier/method, before "From :"
    let re = Regex::new(r"(?i)Shipped\s+per\s*:\s*(.+?)(?:\s{2,}|From\s*:|\n)").ok()?;
    re.captures(text).map(|c| c[1].trim().to_string())
}

/// Find company-like names (X PTE LTD, X CO. LTD, X CO., LTD, etc.)
fn extract_company_names(text: &str) -> Vec<String> {
    let re = Regex::new(
        r"(?i)([A-Z][A-Z\.\s&]+(?:PTE\.?\s*LTD\.?|CO\.?,?\s*LTD\.?|CORPORATION|CORP\.?|INC\.?))",
    )
    .unwrap();
    re.captures_iter(text)
        .map(|c| c[1].trim().to_string())
        .collect()
}

// ---------------------------------------------------------------------------
// Line items extraction
// ---------------------------------------------------------------------------

fn extract_line_items(text: &str) -> Vec<LineItem> {
    let mut items = Vec::new();

    // Strategy: find all number clusters that look like qty + unit_price + amount
    // near product descriptions. The product descriptions contain platform tags
    // like "PS5", "NS", "PS4", "XBOX", "PC", "SWITCH".
    //
    // We scan for description lines, then collect the associated numbers.

    let packing_pos = text
        .to_uppercase()
        .find("PACKING LIST")
        .unwrap_or(text.len());
    let invoice_section = &text[..packing_pos];

    // Find product description lines (contain platform identifiers or known patterns)
    let desc_re =
        Regex::new(r"(?i)([A-Z][A-Z0-9\s\-:&']+(?:PS[45]\s*\w*|NS\s*\w*|SWITCH|XBOX|PC|ASI\w*)\b)")
            .unwrap();

    // Find number groups: qty (integer), unit price (decimal), amount (decimal)
    // They appear as sequences like "100  PIECE  2540.00" ... "25.40"
    let qty_re = Regex::new(r"\b(\d{1,6})\s+PIECE").unwrap();
    let amount_re = Regex::new(r"(\d[\d,]*\.\d{2})").unwrap();

    let descriptions: Vec<String> = desc_re
        .captures_iter(invoice_section)
        .map(|c| c[1].trim().to_string())
        .collect();

    let quantities: Vec<u32> = qty_re
        .captures_iter(invoice_section)
        .filter_map(|c| c[1].parse().ok())
        .collect();

    // Collect all decimal amounts in the invoice section
    let amounts: Vec<f64> = amount_re
        .captures_iter(invoice_section)
        .filter_map(|c| c[1].replace(',', "").parse::<f64>().ok())
        .collect();

    // Match them up: for each description + qty, find the line amount and unit price.
    // Amounts typically come in pairs per item: line total, then unit price
    // or the pattern is: amount ... unit_price near TOTAL
    for (i, desc) in descriptions.iter().enumerate() {
        let qty = quantities.get(i).copied().unwrap_or(0);

        // Find amounts that correspond to this item.
        // Heuristic: amounts > 100 are likely line totals, amounts < 100 likely unit prices
        // when we have small qty items. Better: line_total = qty * unit_price.
        // Try to find a pair where a * b / qty ≈ 1 of the other values.
        let mut item = LineItem {
            description: desc.clone(),
            qty,
            unit_price: 0.0,
            amount: 0.0,
        };

        // Simple approach: look for amounts that divide evenly by qty
        if qty > 0 {
            for &amt in &amounts {
                let candidate_unit = amt / qty as f64;
                // Check if this unit price also appears in the amounts list
                for &other in &amounts {
                    if (other - candidate_unit).abs() < 0.01 && amt != other {
                        item.amount = amt;
                        item.unit_price = candidate_unit;
                        break;
                    }
                }
                if item.amount > 0.0 {
                    break;
                }
            }
        }

        items.push(item);
    }

    items
}

// ---------------------------------------------------------------------------
// Packing list extraction
// ---------------------------------------------------------------------------

fn extract_packing_items(text: &str) -> Vec<PackingItem> {
    let mut items = Vec::new();

    let packing_pos = text.to_uppercase().find("PACKING LIST");
    let Some(pos) = packing_pos else {
        return items;
    };
    let packing_section = &text[pos..];

    // Look for carton rows. The pattern in extracted text is:
    // carton_no  ctns  qty  net_wt  gross_wt  measurement
    // with description on a nearby line.

    // Find product descriptions in the packing section
    let desc_re =
        Regex::new(r"(?i)([A-Z][A-Z0-9\s\-:&']+(?:PS[45]\s*\w*|NS\s*\w*|SWITCH|XBOX|PC|ASI\w*)\b)")
            .unwrap();

    let descriptions: Vec<String> = desc_re
        .captures_iter(packing_section)
        .map(|c| c[1].trim().to_string())
        .collect();

    // Find measurement strings (e.g. "59 X 25 X 20 CM")
    let meas_re = Regex::new(r"(\d+\s*X\s*\d+\s*X\s*\d+\s*CM)").unwrap();
    let measurements: Vec<String> = meas_re
        .captures_iter(packing_section)
        .map(|c| c[1].trim().to_string())
        .collect();

    // Find carton number patterns (e.g. "1", "2-6")
    let carton_re = Regex::new(r"(?m)^\s*(\d+(?:\s*-\s*\d+)?)\s").unwrap();
    // Better: look for the structured rows after CARTON #
    let header_pos = packing_section.to_uppercase().find("CARTON").unwrap_or(0);
    let data_section = &packing_section[header_pos..];

    // Find numeric rows: carton, ctns, qty, net_wt, gross_wt
    let row_re = Regex::new(r"(\d+(?:\s*-\s*\d+)?)\s+(\d+)\s+(\d+)\s+([\d.]+)\s+([\d.]+)").unwrap();

    for (i, cap) in row_re.captures_iter(data_section).enumerate() {
        let item = PackingItem {
            carton: cap[1].trim().to_string(),
            description: descriptions.get(i).cloned().unwrap_or_default(),
            ctns: cap[2].parse().unwrap_or(0),
            qty: cap[3].parse().unwrap_or(0),
            net_wt_per_ctn: cap[4].parse().unwrap_or(0.0),
            gross_wt_per_ctn: cap[5].parse().unwrap_or(0.0),
            measurement: measurements.get(i).cloned().unwrap_or_default(),
        };
        items.push(item);
    }

    items
}

fn extract_packing_totals(text: &str) -> Option<PackingTotals> {
    let packing_pos = text.to_uppercase().find("PACKING LIST")?;
    let packing_section = &text[packing_pos..];

    // Look for the TOTAL row in the packing section
    let total_re = Regex::new(r"(?i)TOTAL\s+(\d+)\s+(\d+)\s+([\d.]+)\s+([\d.]+)").ok()?;

    let cap = total_re.captures(packing_section)?;
    Some(PackingTotals {
        total_cartons: cap[1].parse().unwrap_or(0),
        total_qty: cap[2].parse().unwrap_or(0),
        total_net_wt: cap[3].parse().unwrap_or(0.0),
        total_gross_wt: cap[4].parse().unwrap_or(0.0),
    })
}
