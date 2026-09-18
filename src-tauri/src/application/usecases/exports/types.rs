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

/// Explicit export mode for a Contract Catalog exchange (ADR-0059).
///
/// Locked semantic: the mode is NEVER represented as `Option<TargetUnit>` with
/// `None == FleetRestore`. `UnitDistribution` structurally requires the target
/// node code (`units.code`, authoritative via
/// `transport_target::resolve_unit_transport_target`) and can never represent a
/// missing target; `FleetRestore` is the only fleet-wide form.
///
/// Phase 1 establishes this mode model only; the mode-carrying input wrapper is
/// introduced by the later exporter/use-case wiring phase once constructor
/// boundaries exist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExportContractCatalogMode {
    /// Fleet-wide restore/reissue — UNSCOPED dataset, all statuses retained.
    FleetRestore,
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
            ExportContractCatalogMode::FleetRestore => {
                panic!("UNIT distribution must carry the target unit code");
            }
        }
    }

    #[test]
    fn fleet_restore_has_no_target() {
        let mode = ExportContractCatalogMode::FleetRestore;
        if let ExportContractCatalogMode::UnitDistribution { .. } = mode {
            panic!("fleet restore must not carry a target unit code");
        }
    }
}
