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

/// Consumer-facing input wrapper (ADR-0059 §4, exporter/use-case wiring phase).
///
/// The mode is a mandatory, structurally-explicit field: there is no way to
/// construct a Contract Catalog export without declaring the target `units.code`
/// it is distributed to.
#[derive(Debug, Clone)]
pub struct ExportContractCatalogInput {
    pub mode: ExportContractCatalogMode,
}

/// Explicit export mode for a Contract Catalog exchange (ADR-0059).
///
/// The fleet-wide `FleetRestore` form was retired by ADR-0060; the ONLY
/// Contract Catalog export form is the UNIT-scoped `UnitDistribution`, which
/// structurally requires the target node code (`units.code`, authoritative via
/// `transport_target::resolve_unit_transport_target`) and can never represent a
/// missing target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExportContractCatalogMode {
    /// UNIT-scoped distribution — dataset restricted to the target unit's
    /// exportable contracts (ADR-0059 §6 status policy).
    UnitDistribution { target_unit_code: String },
}

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

// --- Contract fulfillment (ADR-0061; `contract_fulfillment`, UNIT → WILAYA) ---
//
// `FulfillmentFact` is an **allocation-level cumulative state snapshot**, not a
// historical per-order event. It carries no `order_id`, no `order_item_id` and
// no event id: the repository persists no durable historical fulfillment ledger,
// so a per-order attribution could not be proven (ADR-0061 §3). One fact exists
// per allocation, holding the allocation's *current* cumulative fulfilled
// quantity in **purchase units** (ADR-0061 §5).
//
// `fiscal_year`, `purchase_unit` and `conversion_factor` are **cross-check
// only** — they let the destination fail closed, and are never application
// inputs (ADR-0061 §5, §6).

/// Payload schema version of the `FulfillmentFact` contract. Fail-closed: an
/// importer that does not recognise the version rejects the ENTIRE package
/// (ADR-0061 I5, layer 1).
pub const FULFILLMENT_FACT_VERSION: u16 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FulfillmentFact {
    /// The contractual allocation whose cumulative state this fact reports.
    /// Together with `fulfilled_quantity` this is the synchronization identity
    /// (ADR-0061 §2).
    pub allocation_id: String,
    /// ABSOLUTE cumulative fulfilled quantity, in the contract product's
    /// **purchase unit**. Never a delta.
    pub fulfilled_quantity: f64,
    /// The allocation's contractual fiscal year. Cross-check only.
    pub fiscal_year: i32,
    /// Purchase unit code (`domain::units::UnitMeasure`, 1..=10). Cross-check
    /// only — `contract_allocations` itself declares no unit (ADR-0061 §5).
    pub purchase_unit: Option<i32>,
    /// Purchase→consumption conversion factor. Cross-check only. `None` is
    /// treated as factor 1, consistent with `OrderUnitSnapshot::effective_factor`.
    pub conversion_factor: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FulfillmentFactExportDataset {
    pub fact_version: u16,
    /// One fact per eligible allocation, deterministically ordered by
    /// `allocation_id` (freeze §2.5).
    pub facts: Vec<FulfillmentFact>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_distribution_requires_a_target_code() {
        let mode = ExportContractCatalogMode::UnitDistribution {
            target_unit_code: "W101".to_string(),
        };
        match mode {
            ExportContractCatalogMode::UnitDistribution { target_unit_code } => {
                assert_eq!(target_unit_code, "W101");
            }
        }
    }
}
