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
    /// Per-issuer transport sequence (RFC 2026-08-04 §3.4.1). Present on all
    /// accepted packages (SEC-007/ADR-0047: V2-only); `None` is tolerated only
    /// by the schema, never required by the V2 import path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package_sequence: Option<u64>,
    /// Identity of the issuing node (RFC 2026-08-04 §3.4.1). The Transport
    /// Guard keys on `(issuer_identity_id, package_sequence)`.
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
}
