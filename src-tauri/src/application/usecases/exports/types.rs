//! Export usecase types — naming: `*Request` / `*Input` (adapter), `*Dataset` (read model out).

use serde::{Deserialize, Serialize};

use crate::models::{
    Contract, ContractAllocation, ContractAllocationException, DailyDetailSyncSnapshot,
    DailyReportSyncSnapshot, FiscalYearTaxPolicy, MonthlySummary, ProductExportRow, Supplier,
};

// --- Products ----------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct ExportProductsRequest;

#[derive(Debug, Clone, Default)]
pub struct ExportProductsInput;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductsExportDataset {
    pub product_rows: Vec<ProductExportRow>,
}

// --- Daily report -----------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyReportExportDataset {
    pub snapshot: DailyReportSyncSnapshot,
}

#[derive(Debug, Clone)]
pub struct DailyReportExportInput {
    pub report_id: String,
}

// --- Monthly summary --------------------------------------------------------

#[derive(Clone, Copy, Debug)]
pub struct MonthlySummaryExportInput {
    pub year: i32,
    pub month: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonthlySummaryExportDataset {
    pub summary: MonthlySummary,
    pub daily_detail_rows: Vec<DailyDetailSyncSnapshot>,
}

// --- Stock Movements --------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StockMovementsExportDataset {
    pub movements: Vec<crate::models::StockMovement>,
}

#[derive(Debug, Clone)]
pub struct StockMovementsExportInput {
    pub start_date: String,
    pub end_date: String,
}

// --- Contract catalog (ADR-0055 / SEC-087-F; ContractCatalog V2, WILAYA → UNIT) ---
//
// A WILAYA-authoritative read-only projection: suppliers, UNIT↔supplier
// associations, contracts (with product lines + per-UNIT allocations and their
// release exceptions), and the fiscal-year TVA policies. UNIT nodes apply ONLY
// the rows scoped to their own unit id; WILAYA applies the full catalog.

#[derive(Debug, Clone, Default)]
pub struct ExportContractCatalogInput;

/// A contract product line carrying the authoritative ordered-price snapshot
/// plus its `created_at` (both needed by the import upsert; the `ContractProduct`
/// model does not expose `created_at`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractCatalogProductLine {
    pub id: String,
    pub contract_id: String,
    pub product_id: String,
    pub proposed_price_ht: f64,
    pub agreed_price_ht: Option<f64>,
    pub tva_classification: Option<i32>,
    pub tva_rate: Option<f64>,
    pub tva_amount: Option<f64>,
    pub price_ttc: Option<f64>,
    pub purchase_unit: Option<i32>,
    pub consumption_unit: Option<i32>,
    pub conversion_factor: Option<i32>,
    pub created_at: String,
}

/// An allocation carrying its authoritative `created_at` (needed by the import
/// upsert — the `ContractAllocation` model does not expose it).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractCatalogAllocationRow {
    pub allocation: ContractAllocation,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractCatalogContractRow {
    pub contract: Contract,
    pub product_lines: Vec<ContractCatalogProductLine>,
    pub allocations: Vec<ContractCatalogAllocationRow>,
    pub exceptions: Vec<ContractAllocationException>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractCatalogUnitSupplierLink {
    pub unit_id: String,
    pub supplier_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractCatalogExportDataset {
    pub suppliers: Vec<Supplier>,
    pub unit_supplier_links: Vec<ContractCatalogUnitSupplierLink>,
    pub contracts: Vec<ContractCatalogContractRow>,
    pub tax_policies: Vec<FiscalYearTaxPolicy>,
}
