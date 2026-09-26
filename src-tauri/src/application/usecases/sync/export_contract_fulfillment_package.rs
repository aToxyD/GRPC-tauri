//! Produce an ADR-0061 `contract_fulfillment` package on a UNIT node.
//!
//! The dataset is the source UNIT's **complete current cumulative fulfillment
//! state set**: one absolute snapshot per eligible `ContractAllocation` with
//! `fulfilled_quantity > 0`, deterministically ordered by `allocation_id`
//! (freeze §2.5). It is deliberately NOT an event log — the repository persists
//! no historical fulfillment ledger, so a per-order fact could not be proven at
//! the destination (ADR-0061 §3).
//!
//! The package identity is derived from the content itself, so exporting the
//! same state twice yields the same `package_id` and the destination can
//! recognise the repeat (ADR-0061 §4). Everything else — Ed25519
//! `signature_version = 2` signing bound to the node identity, age encryption,
//! integrity hash — is the shared V3 path; no parallel crypto is involved.

use crate::application::sync::PackageId;
use crate::application::usecases::exports::types::{
    FulfillmentFact, FulfillmentFactExportDataset, FULFILLMENT_FACT_VERSION,
};
use crate::errors::{AppError, AppResult, ValidationError};
use crate::infrastructure::sync::packages::content_package_id::derive_content_package_id;
use crate::repositories::{DbExecutor, RepositoryProvider};

/// Build the deterministic dataset and its content-derived `package_id`.
///
/// Exposed separately from the export call so the identity and the payload are
/// provably produced from ONE dataset (A2 — the derivation has a single owner)
/// and so tests can assert the identity contract without touching the
/// filesystem or a node key.
pub fn build_dataset(
    executor: DbExecutor<'_>,
) -> AppResult<(FulfillmentFactExportDataset, PackageId)> {
    let rows = executor.contracts().list_fulfillment_states_for_export()?;
    let facts: Vec<FulfillmentFact> = rows
        .into_iter()
        .map(|row| FulfillmentFact {
            allocation_id: row.allocation_id,
            fulfilled_quantity: row.fulfilled_quantity,
            fiscal_year: row.fiscal_year,
            purchase_unit: row.purchase_unit,
            conversion_factor: row.conversion_factor,
        })
        .collect();

    // Fail closed on an empty set. The destination rejects an empty fact set
    // (`validate_contract_fulfillment_package_for_import`), so producing one
    // would sign, encrypt and persist an artifact that can never be imported —
    // a silently unusable transfer, and a `package_id` that carries no state.
    if facts.is_empty() {
        return Err(AppError::Validation(ValidationError::InvalidFormat {
            field: "facts".into(),
            message: "لا توجد حالات تنفيذ مسجّلة على هذه الوحدة — لا يمكن إنشاء حزمة تنفيذ.".into(),
        }));
    }

    let dataset = FulfillmentFactExportDataset {
        fact_version: FULFILLMENT_FACT_VERSION,
        facts,
    };
    let package_id = derive_content_package_id(&dataset)?;
    Ok((dataset, package_id))
}
