//! Identity authentication policy (B6-A → B6-B).
//!
//! RFC 2026-08-04-node-identity-trust / ADR-0038.
//!
//! Sole decision source for whether the legacy password login path is
//! permitted on the local node. The rule is derived from the SECURITY FACT —
//! an ACTIVE ADMIN identity exists — and NEVER from the derived
//! `IdentityBootstrapState` projection (which is a UI / bootstrap concern only,
//! B6-A refinement 1). A derived state may legitimately evolve; this policy is
//! an independent, testable security rule.
//!
//! B6-B closes the deprecation window: the temporary `GRPC_LEGACY_AUTH` opt-in
//! (Introduced B6-A) is removed and MUST NOT survive. The gate is now
//! PERMANENT — the password path stays open only while no ACTIVE ADMIN identity
//! exists. This preserves Application User Authentication for nodes that have
//! no node identity at all (UNIT local users created from the `.unit` package's
//! `UserExport { username, password_hash, role }`), while WILAYA nodes with an
//! ACTIVE ADMIN identity route exclusively to Operator Authentication
//! (Challenge–Response / `.adminkey`).

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

    /// Whether the legacy password login path is permitted on this node.
    ///
    /// Permanent gate (B6-B): the password path is permitted ONLY while no
    /// ACTIVE ADMIN identity exists. The temporary `GRPC_LEGACY_AUTH` override
    /// that could re-open the path during the B6-A window is removed.
    pub fn password_login_allowed(
        db: &Database,
        adminkey_provider: &AdminKeyProvider,
    ) -> AppResult<bool> {
        Ok(!Self::has_active_admin_identity(db, adminkey_provider)?)
    }
}
