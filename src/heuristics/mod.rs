// src/heuristics/mod.rs

mod generic;

use serde::Deserialize;
use serde::Serialize;

/// A single invoice line item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineItem {
    pub description: String,
    pub qty: u32,
    pub unit_price: f64,
    pub amount: f64,
}

/// A single row from the packing list.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackingItem {
    pub carton: String,
    pub description: String,
    pub ctns: u32,
    pub qty: u32,
    pub net_wt_per_ctn: f64,
    pub gross_wt_per_ctn: f64,
    pub measurement: String,
}

/// Packing list totals.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackingTotals {
    pub total_cartons: u32,
    pub total_qty: u32,
    pub total_net_wt: f64,
    pub total_gross_wt: f64,
}

/// All structured data we can extract from an invoice PDF.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvoiceData {
    pub vendor: Option<String>,
    pub buyer: Option<String>,
    pub invoice_no: Option<String>,
    pub invoice_date: Option<String>,
    pub currency: Option<String>,
    pub total_amount: Option<f64>,
    pub total_pieces: Option<u32>,
    pub ship_from: Option<String>,
    pub ship_to: Option<String>,
    pub shipping_method: Option<String>,
    pub line_items: Vec<LineItem>,
    pub packing_items: Vec<PackingItem>,
    pub packing_totals: Option<PackingTotals>,
}

impl InvoiceData {
    /// How many fields were successfully extracted (out of the scalar ones).
    pub fn coverage(&self) -> (usize, usize) {
        let total = 10;
        let filled = [
            self.vendor.is_some(),
            self.buyer.is_some(),
            self.invoice_no.is_some(),
            self.invoice_date.is_some(),
            self.currency.is_some(),
            self.total_amount.is_some(),
            self.total_pieces.is_some(),
            self.ship_from.is_some(),
            self.ship_to.is_some(),
            self.shipping_method.is_some(),
        ]
        .iter()
        .filter(|&&v| v)
        .count();
        (filled, total)
    }
}

/// Extract structured invoice data from raw PDF text.
pub fn extract_invoice(text: &str) -> InvoiceData {
    generic::extract(text)
}
