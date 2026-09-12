//! Contract Models
//!
//! Contract is the pricing + entitlement authority (ADR-0055 / SEC-087-F).
//! A Contract is scoped to one UNIT + one Supplier + one fiscal year.
//! Per-UNIT obligations live on `ContractAllocation`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Contract lifecycle status
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ContractStatus {
    Proposed,
    Accepted,
    Active,
    Ended,
    Cancelled,
}

impl std::fmt::Display for ContractStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ContractStatus::Proposed => write!(f, "proposed"),
            ContractStatus::Accepted => write!(f, "accepted"),
            ContractStatus::Active => write!(f, "active"),
            ContractStatus::Ended => write!(f, "ended"),
            ContractStatus::Cancelled => write!(f, "cancelled"),
        }
    }
}

impl From<String> for ContractStatus {
    fn from(s: String) -> Self {
        match s.as_str() {
            "accepted" => ContractStatus::Accepted,
            "active" => ContractStatus::Active,
            "ended" => ContractStatus::Ended,
            "cancelled" => ContractStatus::Cancelled,
            _ => ContractStatus::Proposed,
        }
    }
}

impl ContractStatus {
    pub fn can_accept(&self) -> bool {
        matches!(self, ContractStatus::Proposed)
    }

    pub fn can_activate(&self) -> bool {
        matches!(self, ContractStatus::Accepted | ContractStatus::Active)
    }

    pub fn can_end(&self) -> bool {
        matches!(self, ContractStatus::Active | ContractStatus::Accepted)
    }

    /// proposed / accepted / active may transition to cancelled.
    pub fn can_cancel(&self) -> bool {
        matches!(
            self,
            ContractStatus::Proposed | ContractStatus::Accepted | ContractStatus::Active
        )
    }
}

/// Contract header
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contract {
    pub id: String,
    pub contract_reference: String,
    pub unit_id: String,
    pub supplier_id: String,
    pub fiscal_year: i32,
    pub status: ContractStatus,
    pub proposed_at: Option<String>,
    pub accepted_at: Option<String>,
    pub activated_at: Option<String>,
    pub ended_at: Option<String>,
    pub cancelled_at: Option<String>,
    pub notes: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
}

/// Product line of a contract (pricing authority).
///
/// SEC-087 Phase 3: carries the authoritative ordered-price snapshot persisted
/// at agreement: `proposed_price_ht` (supplier proposal), `agreed_price_ht`
/// (WILAYA-approved contractual HT), `tva_*`, and `price_ttc` (TTC per purchase
/// unit) plus the purchase/consumption unit split and conversion factor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractProduct {
    pub id: String,
    pub contract_id: String,
    pub product_id: String,
    pub product_name: String,
    /// Supplier-proposed HT price per purchase unit.
    pub proposed_price_ht: f64,
    /// Authoritative WILAYA-agreed HT per purchase unit (SEC-087 Phase 3).
    pub agreed_price_ht: Option<f64>,
    /// Product's TVA classification code (`domain::units::TvaClassification`).
    pub tva_classification: Option<i32>,
    /// Exact TVA rate (percent domain, scale-4 persisted as INTEGER).
    pub tva_rate: Option<f64>,
    /// Rounded TVA term `round_2dp(HT × rate / 100)`.
    pub tva_amount: Option<f64>,
    /// Authoritative TTC per purchase unit; drives the resolver.
    pub price_ttc: Option<f64>,
    /// Purchase unit code (`domain::units::UnitMeasure`, 1..=10).
    pub purchase_unit: Option<i32>,
    /// Consumption unit code (`domain::units::UnitMeasure`, 1..=10).
    pub consumption_unit: Option<i32>,
    /// Purchase→consumption conversion factor (==1 when units identical).
    pub conversion_factor: Option<i32>,
}

/// Per-UNIT obligation/entitlement row
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractAllocation {
    pub id: String,
    pub contract_id: String,
    pub contract_product_id: String,
    pub unit_id: String,
    pub product_id: String,
    pub fiscal_year: i32,
    pub contracted_quantity: f64,
    pub fulfilled_quantity: f64,
    pub released_quantity: f64,
    pub reserved_quantity: f64,
    pub entitlement_state: String,
    pub version: i64,
}

/// Single-source derivation of the component-based remaining quantity
/// (A5/P2). Not stored authoritatively; owned and computed only here so
/// no layer re-derives obligation arithmetic.
pub fn effective_remaining(contracted: f64, fulfilled: f64, released: f64, reserved: f64) -> f64 {
    contracted - fulfilled - released - reserved
}

impl ContractAllocation {
    /// Derived component-based remaining (not stored authoritatively).
    pub fn effective_remaining(&self) -> f64 {
        effective_remaining(
            self.contracted_quantity,
            self.fulfilled_quantity,
            self.released_quantity,
            self.reserved_quantity,
        )
    }
}

/// Backend-derived obligation projection consumed by the frontend.
///
/// `effective_remaining` is computed in the domain model (single owner, A5/P2)
/// and serialized so presentation layers never re-derive business arithmetic.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractAllocationView {
    pub id: String,
    pub contract_id: String,
    pub contract_product_id: String,
    pub unit_id: String,
    pub product_id: String,
    pub fiscal_year: i32,
    pub contracted_quantity: f64,
    pub fulfilled_quantity: f64,
    pub released_quantity: f64,
    pub reserved_quantity: f64,
    pub entitlement_state: String,
    pub version: i64,
    pub effective_remaining: f64,
}

impl From<ContractAllocation> for ContractAllocationView {
    fn from(allocation: ContractAllocation) -> Self {
        let effective_remaining = allocation.effective_remaining();
        Self {
            id: allocation.id,
            contract_id: allocation.contract_id,
            contract_product_id: allocation.contract_product_id,
            unit_id: allocation.unit_id,
            product_id: allocation.product_id,
            fiscal_year: allocation.fiscal_year,
            contracted_quantity: allocation.contracted_quantity,
            fulfilled_quantity: allocation.fulfilled_quantity,
            released_quantity: allocation.released_quantity,
            reserved_quantity: allocation.reserved_quantity,
            entitlement_state: allocation.entitlement_state,
            version: allocation.version,
            effective_remaining,
        }
    }
}

/// Raw row projection of a local ContractCatalog allocation that the UNIT
/// consumes. `effective_remaining` is intentionally NOT carried here: it is
/// computed in the application service via the single-source domain helper
/// (`effective_remaining`), so the repository performs no arithmetic.
#[derive(Debug, Clone)]
pub struct UnitEntitlementRow {
    pub product_id: String,
    pub product_name: String,
    pub supplier_id: String,
    pub supplier_name: String,
    pub fiscal_year: i32,
    pub contracted_quantity: f64,
    pub fulfilled_quantity: f64,
    pub released_quantity: f64,
    pub reserved_quantity: f64,
    pub entitlement_state: String,
    pub contract_status: String,
    pub price_ttc: Option<f64>,
}

/// Read-only UNIT entitlement projection DTO (Phase 4). Backend-derived:
/// `effective_remaining` is computed from the domain helper and serialized so
/// the frontend never re-derives obligation arithmetic (A5/P2).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnitContractEntitlement {
    pub product_id: String,
    pub product_name: String,
    pub supplier_id: String,
    pub supplier_name: String,
    pub fiscal_year: i32,
    pub contracted_quantity: f64,
    pub fulfilled_quantity: f64,
    pub released_quantity: f64,
    pub reserved_quantity: f64,
    pub effective_remaining: f64,
    pub entitlement_state: String,
    pub contract_status: String,
    pub price_ttc: Option<f64>,
}

impl From<UnitEntitlementRow> for UnitContractEntitlement {
    fn from(row: UnitEntitlementRow) -> Self {
        let effective_remaining = effective_remaining(
            row.contracted_quantity,
            row.fulfilled_quantity,
            row.released_quantity,
            row.reserved_quantity,
        );
        Self {
            product_id: row.product_id,
            product_name: row.product_name,
            supplier_id: row.supplier_id,
            supplier_name: row.supplier_name,
            fiscal_year: row.fiscal_year,
            contracted_quantity: row.contracted_quantity,
            fulfilled_quantity: row.fulfilled_quantity,
            released_quantity: row.released_quantity,
            reserved_quantity: row.reserved_quantity,
            effective_remaining,
            entitlement_state: row.entitlement_state,
            contract_status: row.contract_status,
            price_ttc: row.price_ttc,
        }
    }
}

/// Reason codes for the WILAYA-only obligation release exception.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ReleaseReasonCode {
    SupplierNonPerformance,
    SupplierDelay,
    ServiceContinuity,
    OtherAuthorized,
}

impl std::fmt::Display for ReleaseReasonCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReleaseReasonCode::SupplierNonPerformance => write!(f, "SUPPLIER_NON_PERFORMANCE"),
            ReleaseReasonCode::SupplierDelay => write!(f, "SUPPLIER_DELAY"),
            ReleaseReasonCode::ServiceContinuity => write!(f, "SERVICE_CONTINUITY"),
            ReleaseReasonCode::OtherAuthorized => write!(f, "OTHER_AUTHORIZED"),
        }
    }
}

impl From<String> for ReleaseReasonCode {
    fn from(s: String) -> Self {
        match s.as_str() {
            "SUPPLIER_NON_PERFORMANCE" => ReleaseReasonCode::SupplierNonPerformance,
            "SUPPLIER_DELAY" => ReleaseReasonCode::SupplierDelay,
            "SERVICE_CONTINUITY" => ReleaseReasonCode::ServiceContinuity,
            _ => ReleaseReasonCode::OtherAuthorized,
        }
    }
}

/// WILAYA-authorized release record (auditable, transactional).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractAllocationException {
    pub id: String,
    pub allocation_id: String,
    pub released_quantity: f64,
    pub reason_code: String,
    pub reason_note: Option<String>,
    pub created_by: String,
    pub created_at: String,
}

/// Request to create a contract header (WILAYA only)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateContractRequest {
    pub unit_id: String,
    pub supplier_id: String,
    pub fiscal_year: i32,
    pub contract_reference: String,
    pub notes: Option<String>,
}

/// Add a product to a contract: creates contract_products + allocation row.
/// `proposed_price_ht` is the supplier proposal; `agreed_price_ht` may be set
/// later during the proposed phase via `SetAgreedPriceHtRequest`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddContractProductRequest {
    pub contract_id: String,
    pub product_id: String,
    pub proposed_price_ht: f64,
    pub agreed_price_ht: Option<f64>,
    pub contracted_quantity: f64,
}

/// Approve/freeze the agreed HT price for a contract product (WILAYA). Allowed
/// only while the contract is `proposed`; after acceptance it is immutable.
///
/// SEC-087 Phase 3: `agreed_price_ht` is the authoritative HT per purchase
/// unit; the persisted snapshot (HT + TVA + TTC) is computed here.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetAgreedPriceHtRequest {
    pub contract_product_id: String,
    pub agreed_price_ht: f64,
}

/// Transition requests
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractTransitionRequest {
    pub contract_id: String,
}

/// WILAYA-only obligation release
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseContractAllocationRequest {
    pub allocation_id: String,
    pub released_quantity: f64,
    pub reason_code: ReleaseReasonCode,
    pub reason_note: Option<String>,
}

/// Revoke a prior release (only while safe)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevokeContractAllocationReleaseRequest {
    pub exception_id: String,
}

/// Authoritative ordered-price snapshot persisted at the price-agreement
/// boundary (SEC-087 Phase 3). All monetary fields are pre-scaled INTEGERs and
/// unit/TVA fields are closed-set codes, produced by the application layer from
/// exact `Money`/`Rate` arithmetic; the repository performs no arithmetic.
#[derive(Debug, Clone)]
pub struct ContractPriceSnapshot {
    pub contract_product_id: String,
    pub agreed_price_ht_scaled: i64,
    pub tva_classification_code: i32,
    pub tva_rate_scaled: i64,
    pub tva_amount_scaled: i64,
    pub price_ttc_scaled: i64,
    pub purchase_unit_code: i32,
    pub consumption_unit_code: i32,
    pub conversion_factor: i32,
}

/// Read-only product unit/TVA configuration codes (SEC-087) needed to build an
/// authoritative contract price snapshot. Fields are REQUIRED codes (`i32`) —
/// the canonical schema enforces NOT NULL, so a persisted Product always
/// carries all four.
#[derive(Debug, Clone, Copy)]
pub struct ProductUnitConfigCodes {
    pub purchase_unit: i32,
    pub consumption_unit: i32,
    pub conversion_factor: i32,
    pub tva_classification: i32,
}
