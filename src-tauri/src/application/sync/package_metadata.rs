//! Provenance and versioning for sync packages (transport-agnostic).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::SchemaVersion;

/// Stable identifier for a package instance (import idempotency / replay avoidance later).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PackageId(pub String);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncPackageMetadata {
    pub created_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub integrity_hash: Option<String>,
    pub package_id: PackageId,
    pub schema_version: SchemaVersion,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature_version: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signing_key_id: Option<String>,
    pub source_node_id: String,
}
