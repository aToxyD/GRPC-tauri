//! Identity authentication policy (B6-A).
//!
//! RFC 2026-08-04-node-identity-trust / ADR-0038.
//!
//! Sole decision source for whether the legacy password login path is
//! permitted on the local node. The rule is derived from the SECURITY FACT —
//! an ACTIVE ADMIN identity exists — and NEVER from the derived
//! `IdentityBootstrapState` projection (which is a UI / bootstrap concern only,
//! B6-A refinement 1). A derived state may legitimately evolve; this policy is
//! an independent, testable security rule.

use crate::db::Database;
use crate::domain::identity::{IdentityStorePort, SubjectType};
use crate::errors::AppResult;
use crate::infrastructure::identity::AdminKeyProvider;
use crate::repositories::RepositoryProvider;

/// Authentication method policy for the local node.
pub struct IdentityAuthenticationPolicy;

impl IdentityAuthenticationPolicy {
    /// An ACTIVE ADMIN certificate EXISTS in the Identity Store AND the
    /// portable `.adminkey` is present on disk.
    ///
    /// Both halves must hold. Requiring `.adminkey` presence is the fail-safe
    /// against a lockout: a certificate without key material must not disable
    /// the operator's only remaining login path (B6-A refinement 1).
    pub fn has_active_admin_identity(
        db: &Database,
        adminkey_provider: &AdminKeyProvider,
    ) -> AppResult<bool> {
        let store = db.executor().identity_store();
        let admin_cert = store.get_active_by_subject_type(SubjectType::Admin)?;
        Ok(admin_cert.is_some() && adminkey_provider.exists())
    }

    /// Whether the legacy password login path is permitted, honoring the
    /// process environment (`GRPC_LEGACY_AUTH`).
    pub fn password_login_allowed(
        db: &Database,
        adminkey_provider: &AdminKeyProvider,
    ) -> AppResult<bool> {
        Self::password_login_allowed_with_override(db, adminkey_provider, legacy_auth_override())
    }

    /// Pure decision, override threaded explicitly so the gate matrix is
    /// unit-testable without mutating the process environment (P1).
    pub fn password_login_allowed_with_override(
        db: &Database,
        adminkey_provider: &AdminKeyProvider,
        legacy_auth_override: bool,
    ) -> AppResult<bool> {
        Ok(!Self::has_active_admin_identity(db, adminkey_provider)? || legacy_auth_override)
    }
}

/// `GRPC_LEGACY_AUTH` opt-in (`1` enables).
///
/// Temporary compatibility override.
/// Introduced: B6-A
/// Removed: B6-B
/// MUST NOT survive after B6-B.
fn legacy_auth_override() -> bool {
    match std::env::var("GRPC_LEGACY_AUTH") {
        Ok(v) => v.trim() == "1",
        Err(_) => false,
    }
}
