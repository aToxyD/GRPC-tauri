//! Licensing consumer contract types and DTOs.
//!
//! ADR-0042 / artifact-spec v1. These types mirror the signed artifact envelope
//! (§2) and the provisioning package (§9) as plain data — structural validation
//! happens in `infrastructure/licensing`, semantic validation (type registry,
//! entitlement mapping) in `application/licensing`. DTOs are the command
//! surface used by `commands/licensing.rs`.

use serde::{Deserialize, Serialize};

/// Signed licensing artifact envelope (artifact-spec §2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedLicenseArtifact {
    pub version: u32,
    pub metadata: ArtifactMetadata,
    pub payload: ArtifactPayload,
    pub signature: ArtifactSignatureRef,
}

/// Envelope metadata block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactMetadata {
    pub key_id: String,
    pub algorithm: String,
    pub artifact_id: String,
    pub issued_for: String,
}

/// Envelope payload block (ARTIFACT_MODEL.md §2 — 7 canonical components).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactPayload {
    pub license: LicensePayload,
    pub subject: SubjectPayload,
    pub entitlements: Vec<String>,
    pub contract_version: ContractVersionPayload,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LicensePayload {
    pub id: String,
    pub type_key: String,
    pub status: String,
}

/// Opaque subject identifier. Per ADR-0042 §4 it is the
/// base64url(URL_SAFE_NO_PAD) encoding of the node's raw 32-byte Ed25519
/// public key — evaluated byte-exact. No other convention is accepted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubjectPayload {
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractVersionPayload {
    pub major: u32,
    pub minor: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactSignatureRef {
    pub algorithm: String,
    pub key_id: String,
    /// base64url(URL_SAFE_NO_PAD) of the raw 64-byte Ed25519 signature.
    pub signature: String,
}

/// Signed license status (LICENSE_LIFECYCLE.md). `grpc` applies the declared
/// state; it never derives temporal state from the wall clock (ADR-0005 §2.7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LicenseStatus {
    Active,
    Suspended,
    Revoked,
    Expired,
}

impl LicenseStatus {
    /// The signed `payload.license.status` key.
    pub fn key(&self) -> &'static str {
        match self {
            LicenseStatus::Active => "active",
            LicenseStatus::Suspended => "suspended",
            LicenseStatus::Revoked => "revoked",
            LicenseStatus::Expired => "expired",
        }
    }

    /// Strict key → status mapping. Unknown ⇒ `None` (fail-closed).
    pub fn from_key(key: &str) -> Option<LicenseStatus> {
        match key {
            "active" => Some(LicenseStatus::Active),
            "suspended" => Some(LicenseStatus::Suspended),
            "revoked" => Some(LicenseStatus::Revoked),
            "expired" => Some(LicenseStatus::Expired),
            _ => None,
        }
    }

    /// Whether the declared state grants entitlements (ADR-0042 §5).
    pub fn is_enforceable(&self) -> bool {
        matches!(self, LicenseStatus::Active)
    }
}

/// Declared entitlement keys (ADR-0042 §5 mapping table). Unknown keys are
/// rejected at import — never ignored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntitlementKey {
    #[serde(rename = "core.auth")]
    CoreAuth,
    #[serde(rename = "core.sync")]
    CoreSync,
    #[serde(rename = "core.reports")]
    CoreReports,
    #[serde(rename = "core.stock")]
    CoreStock,
    #[serde(rename = "core.consume")]
    CoreConsume,
    #[serde(rename = "core.admin")]
    CoreAdmin,
}

impl EntitlementKey {
    /// The declared entitlement string.
    pub fn key(&self) -> &'static str {
        match self {
            EntitlementKey::CoreAuth => "core.auth",
            EntitlementKey::CoreSync => "core.sync",
            EntitlementKey::CoreReports => "core.reports",
            EntitlementKey::CoreStock => "core.stock",
            EntitlementKey::CoreConsume => "core.consume",
            EntitlementKey::CoreAdmin => "core.admin",
        }
    }

    /// Strict key → entitlement mapping. Unknown ⇒ `None` (fail-closed).
    pub fn from_key(key: &str) -> Option<EntitlementKey> {
        match key {
            "core.auth" => Some(EntitlementKey::CoreAuth),
            "core.sync" => Some(EntitlementKey::CoreSync),
            "core.reports" => Some(EntitlementKey::CoreReports),
            "core.stock" => Some(EntitlementKey::CoreStock),
            "core.consume" => Some(EntitlementKey::CoreConsume),
            "core.admin" => Some(EntitlementKey::CoreAdmin),
            _ => None,
        }
    }
}

/// Provisioning package (artifact-spec §9, `provisioning-v1`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProvisioningPackage {
    pub format: String,
    pub key_id: String,
    pub algorithm: String,
    /// base64url(URL_SAFE_NO_PAD) of the raw 32-byte Ed25519 public key.
    pub public_key: String,
}

// ────────────────────────────────────────────────────────────────────────────
// Response DTOs (command surface)
// ────────────────────────────────────────────────────────────────────────────

/// Live anchor projection for `get_licensing_status`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustAnchorView {
    pub installed: bool,
    pub key_id: Option<String>,
}

/// Per-license derived view for `get_licensing_status` / `verify_license`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LicenseView {
    pub license_id: String,
    pub artifact_id: String,
    pub type_key: String,
    pub subject_id: String,
    pub entitlements: Vec<String>,
    pub status: String,
    pub contract_version_major: u32,
    pub contract_version_minor: u32,
    pub imported_at: String,
    pub last_verified_at: Option<String>,
    /// Active + re-verifies valid (full §3 pipeline) + node-bound.
    pub enforceable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LicensingSummary {
    pub anchor_installed: bool,
    pub license_count: u64,
    pub active_license_count: u64,
    /// Dormant until an active anchor is installed (ADR-0042 §5).
    pub gate_active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LicensingStatus {
    pub anchor: TrustAnchorView,
    pub licenses: Vec<LicenseView>,
    pub summary: LicensingSummary,
}

/// Result of `import_trust_anchor` (install/replace the single active anchor).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportAnchorResult {
    pub installed: bool,
    pub key_id: String,
    /// The previous active anchor's key_id, when this import rotated it.
    pub replaced_key_id: Option<String>,
}

/// Result of `import_license`. `outcome` is one of
/// `imported` | `rejected` | `not-for-this-node` (ADR-0042 §4 distinction:
/// a valid signature with failed binding is NOT `rejected`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportLicenseResult {
    pub outcome: String,
    pub license_id: Option<String>,
    pub artifact_id: Option<String>,
    pub message: String,
}

/// Report of `verify_license` (deterministic re-verification, no re-import).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifyLicenseReport {
    pub checked: u64,
    pub enforceable: u64,
    pub results: Vec<LicenseView>,
}
