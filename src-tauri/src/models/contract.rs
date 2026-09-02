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

/// Product line of a contract (pricing authority)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractProduct {
    pub id: String,
    pub contract_id: String,
    pub product_id: String,
    pub product_name: String,
    pub proposed_price: f64,
    pub agreed_price: Option<f64>,
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

impl ContractAllocation {
    /// Derived component-based remaining (not stored authoritatively).
    pub fn effective_remaining(&self) -> f64 {
        self.contracted_quantity
            - self.fulfilled_quantity
            - self.released_quantity
            - self.reserved_quantity
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
/// `agreed_price` may be set later during the proposed phase.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddContractProductRequest {
    pub contract_id: String,
    pub product_id: String,
    pub proposed_price: f64,
    pub agreed_price: Option<f64>,
    pub contracted_quantity: f64,
}

/// Approve/freeze the agreed price for a contract product (WILAYA). Allowed
/// only while the contract is `proposed`; after acceptance it is immutable.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetAgreedPriceRequest {
    pub contract_product_id: String,
    pub agreed_price: f64,
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
