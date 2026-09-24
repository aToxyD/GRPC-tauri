//! Provenance and versioning for sync packages (transport-agnostic).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::SchemaVersion;

/// Stable identifier for a package instance (import idempotency / replay avoidance later).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PackageId(pub String);

/// Authenticated export mode of a Contract Catalog sync package (SEC-087
/// Phase 2). Carried inside the Ed25519-signed metadata so import bindings
/// never infer a mode from the mere presence/absence of a target node id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PackageExportMode {
    /// Fleet-wide Contract Catalog restore/reissue (dataset unscoped).
    FleetRestore,
    /// UNIT-scoped distribution (dataset restricted to `target_node_id`).
    UnitDistribution,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncPackageMetadata {
    pub created_at: DateTime<Utc>,
    /// Authenticated export mode. `None` for every non-Contract-Catalog package
    /// kind and for legacy/metadata-only reads (SEC-087 Phase 2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub export_mode: Option<PackageExportMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub integrity_hash: Option<String>,
    /// Identity of the issuing node (RFC 2026-08-04 §3.4.1). Resolved via the
    /// Identity Store to authenticate the Ed25519 package signature (SEC-056D/
    /// SEC-057 removed the per-issuer transport sequence; authenticity is
    /// carried by this id + signature alone).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issuer_identity_id: Option<uuid::Uuid>,
    pub package_id: PackageId,
    pub schema_version: SchemaVersion,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature_version: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signing_key_id: Option<String>,
    pub source_node_id: String,
    /// Target UNIT node code for `UnitDistribution` exports. `None` for fleet
    /// restores and every non-Contract-Catalog package kind (SEC-087 Phase 2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_node_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::sync::SyncPackage;
    use crate::infrastructure::sync::packages::canonical_json::{
        canonical_bytes_for_integrity, canonical_bytes_for_signature,
    };
    use serde_json::{json, Value};

    fn base_metadata() -> SyncPackageMetadata {
        SyncPackageMetadata {
            created_at: Utc::now(),
            export_mode: None,
            integrity_hash: None,
            issuer_identity_id: None,
            package_id: PackageId("pkg".into()),
            schema_version: super::SchemaVersion::V3,
            signature: None,
            signature_version: None,
            signing_key_id: None,
            source_node_id: "node-a".into(),
            target_node_id: None,
        }
    }

    #[test]
    fn package_export_mode_round_trips_snake_case() {
        let fleet = serde_json::to_value(PackageExportMode::FleetRestore).unwrap();
        assert_eq!(fleet, json!("fleet_restore"));
        let unit = serde_json::to_value(PackageExportMode::UnitDistribution).unwrap();
        assert_eq!(unit, json!("unit_distribution"));

        let parsed: PackageExportMode = serde_json::from_value(json!("fleet_restore")).unwrap();
        assert_eq!(parsed, PackageExportMode::FleetRestore);
        let parsed: PackageExportMode = serde_json::from_value(json!("unit_distribution")).unwrap();
        assert_eq!(parsed, PackageExportMode::UnitDistribution);
    }

    #[test]
    fn absent_authenticated_fields_are_omitted_from_wire_shape() {
        let value = serde_json::to_value(base_metadata()).unwrap();
        let obj = value.as_object().unwrap();
        assert!(
            !obj.contains_key("export_mode"),
            "None export_mode must be omitted"
        );
        assert!(
            !obj.contains_key("target_node_id"),
            "None target_node_id must be omitted"
        );
    }

    #[test]
    fn present_authenticated_fields_are_serialized() {
        let mut meta = base_metadata();
        meta.export_mode = Some(PackageExportMode::UnitDistribution);
        meta.target_node_id = Some("UNIT-A".into());
        let value = serde_json::to_value(&meta).unwrap();
        assert_eq!(value["export_mode"], json!("unit_distribution"));
        assert_eq!(value["target_node_id"], json!("UNIT-A"));
    }

    #[test]
    fn legacy_json_without_authenticated_fields_defaults_to_none() {
        let value = json!({
            "created_at": "2026-01-01T00:00:00Z",
            "package_id": "pkg",
            "schema_version": 3,
            "source_node_id": "node-a"
        });
        let parsed: SyncPackageMetadata = serde_json::from_value(value).unwrap();
        assert_eq!(parsed.export_mode, None);
        assert_eq!(parsed.target_node_id, None);
    }

    #[test]
    fn authenticated_fields_change_canonical_integrity_and_signature_bytes() {
        let none_pkg = SyncPackage {
            metadata: base_metadata(),
            payload: Value::Null,
        };
        let mut some_meta = base_metadata();
        some_meta.export_mode = Some(PackageExportMode::UnitDistribution);
        some_meta.target_node_id = Some("UNIT-A".into());
        let some_pkg = SyncPackage {
            metadata: some_meta,
            payload: Value::Null,
        };

        let none_value = serde_json::to_value(&none_pkg).unwrap();
        let some_value = serde_json::to_value(&some_pkg).unwrap();

        let none_integrity = canonical_bytes_for_integrity(&none_value).unwrap();
        let some_integrity = canonical_bytes_for_integrity(&some_value).unwrap();
        assert_ne!(
            none_integrity, some_integrity,
            "export_mode/target_node_id must be covered by the integrity hash canonical bytes"
        );

        let none_signature = canonical_bytes_for_signature(&none_value).unwrap();
        let some_signature = canonical_bytes_for_signature(&some_value).unwrap();
        assert_ne!(
            none_signature, some_signature,
            "export_mode/target_node_id must be covered by the signature canonical bytes"
        );
    }
}
