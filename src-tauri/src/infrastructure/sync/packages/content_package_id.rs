//! Content-derived package identity for ADR-0061 `contract_fulfillment`.
//!
//! Every other kind derives its `package_id` from a fresh random UUID at
//! export time, which makes the id a property of the *export event* rather
//! than of the exported content. ADR-0061 §4 requires the opposite for
//! fulfillment: the same current cumulative state MUST produce the same
//! identity, so a re-export of an unchanged state set is recognised as a
//! duplicate instead of appearing as a new fact stream.
//!
//! The identity is therefore
//!
//! ```text
//! package_id = hex(sha256(canonical_json(dataset)))
//! ```
//!
//! Canonical JSON (sorted keys, no insignificant whitespace, ADR-0009) is used
//! so the derivation is independent of struct field order and of the writer.
//!
//! This module is the ONLY place the derivation lives: the producer calls
//! `derive_content_package_id` when building the package and the consumer
//! recomputes it on import to fail closed when a `package_id` does not
//! describe its own content (A2 — single owner for the derivation).
//!
//! Note on floats: `serde_json` emits the shortest round-trip representation
//! of an `f64`, so a value read from SQLite (scaled integer → `f64`) and the
//! same value parsed back from the package bytes are bit-identical and
//! canonicalize to identical bytes.

use serde::Serialize;

use crate::application::sync::PackageId;
use crate::errors::{AppError, AppResult};

use super::canonical_json::canonical_json_bytes;
use super::integrity::{PackageHasher, Sha256PackageHasher};

/// Derive the content-addressed `package_id` of a sync payload.
pub fn derive_content_package_id<T: Serialize>(payload: &T) -> AppResult<PackageId> {
    let value = serde_json::to_value(payload)
        .map_err(|e| AppError::Internal(format!("sync JSON to_value: {}", e)))?;
    let bytes = canonical_json_bytes(&value)?;
    let hash = Sha256PackageHasher.hash(&bytes)?;
    Ok(PackageId(hash))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::usecases::exports::types::{
        FulfillmentFact, FulfillmentFactExportDataset, FULFILLMENT_FACT_VERSION,
    };

    fn dataset(qty: f64) -> FulfillmentFactExportDataset {
        FulfillmentFactExportDataset {
            fact_version: FULFILLMENT_FACT_VERSION,
            facts: vec![FulfillmentFact {
                allocation_id: "AL-1".into(),
                fulfilled_quantity: qty,
                fiscal_year: 2026,
                purchase_unit: Some(1),
                conversion_factor: None,
            }],
        }
    }

    #[test]
    fn derivation_is_stable_across_calls() {
        assert_eq!(
            derive_content_package_id(&dataset(10.0)).unwrap(),
            derive_content_package_id(&dataset(10.0)).unwrap()
        );
    }

    #[test]
    fn derivation_changes_with_state() {
        assert_ne!(
            derive_content_package_id(&dataset(10.0)).unwrap(),
            derive_content_package_id(&dataset(12.5)).unwrap()
        );
    }

    #[test]
    fn derivation_is_lowercase_hex_sha256() {
        let id = derive_content_package_id(&dataset(10.0)).unwrap();
        assert_eq!(id.0.len(), 64);
        assert!(id
            .0
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_uppercase()));
    }

    #[test]
    fn derivation_round_trips_through_the_wire_format() {
        // Producer → canonical bytes → wire JSON → consumer must agree, which is
        // what makes the destination-side recomputation meaningful.
        let produced = dataset(7.25);
        let wire = serde_json::to_string(&produced).unwrap();
        let received: FulfillmentFactExportDataset = serde_json::from_str(&wire).unwrap();
        assert_eq!(
            derive_content_package_id(&produced).unwrap(),
            derive_content_package_id(&received).unwrap()
        );
    }
}
