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

/// State of the node's canonical ADMIN identity with respect to the credential
/// material the node can actually authenticate with (SEC-002-R).
///
/// This is the SINGLE source of truth consumed by BOTH the password login gate
/// and the first-admin / recovery ceremony, so the two can never disagree on
/// whether the ADMIN identity is usable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdminCredentialState {
    /// No ACTIVE ADMIN certificate — the node is unprovisioned and first-admin
    /// bootstrap (pre-auth) applies.
    NoActiveAdmin,
    /// ACTIVE ADMIN certificate AND a `.adminkey` embedding exactly that
    /// certificate — the ADMIN identity is fully usable.
    Usable,
    /// ACTIVE ADMIN certificate present, but no `.adminkey` on disk.
    MissingAdminkey,
    /// ACTIVE ADMIN certificate present, and the `.adminkey` embeds a DIFFERENT
    /// certificate.
    MismatchedAdminkey,
}

/// Authentication method policy for the local node.
pub struct IdentityAuthenticationPolicy;

impl IdentityAuthenticationPolicy {
    /// Classify the node's canonical ADMIN credential state (SEC-002-R).
    ///
    /// The identity is USABLE only when the ACTIVE ADMIN certificate AND a
    /// `.adminkey` embedding that exact certificate (same identity facts,
    /// `is_identical_to` — the same certificate-match semantic the
    /// Challenge–Response path verifies) are both present.
    ///
    /// Fail-closed: a corrupt/unreadable `.adminkey` surfaces as an error and
    /// MUST NOT be conflated with "missing". A `.adminkey` whose embedded
    /// certificate differs from the stored ACTIVE certificate is NOT usable:
    /// treating file *presence* alone as "active identity" would deadlock the
    /// operator (password login blocked while recovery re-issues inside the
    /// same transaction) and would keep the recovery ceremony open to
    /// unauthenticated callers.
    pub fn admin_credential_state(
        db: &Database,
        adminkey_provider: &AdminKeyProvider,
    ) -> AppResult<AdminCredentialState> {
        let store = db.executor().identity_store();
        let Some(admin_cert) = store.get_active_by_subject_type(SubjectType::Admin)? else {
            return Ok(AdminCredentialState::NoActiveAdmin);
        };
        let Some(file) = adminkey_provider.read_if_exists()? else {
            return Ok(AdminCredentialState::MissingAdminkey);
        };
        if file.certificate.is_identical_to(&admin_cert) {
            Ok(AdminCredentialState::Usable)
        } else {
            Ok(AdminCredentialState::MismatchedAdminkey)
        }
    }

    /// Whether the ADMIN identity is usable for authentication on this node.
    ///
    /// SEC-002-R (B): true ONLY for `AdminCredentialState::Usable` — an ACTIVE
    /// ADMIN certificate EXISTS in the Identity Store AND the portable
    /// `.adminkey` is present AND embeds exactly that certificate.
    ///
    /// All other states disable the identity fact so the operator is never
    /// locked out: a certificate without matching key material must not disable
    /// the operator's only remaining login path (B6-A refinement 1).
    pub fn has_active_admin_identity(
        db: &Database,
        adminkey_provider: &AdminKeyProvider,
    ) -> AppResult<bool> {
        Ok(Self::admin_credential_state(db, adminkey_provider)?
            == AdminCredentialState::Usable)
    }

    /// Whether the legacy password login path is permitted on this node.
    ///
    /// Permanent gate (B6-B): the password path is permitted ONLY while the
    /// ADMIN identity is not usable. The temporary `GRPC_LEGACY_AUTH` override
    /// that could re-open the path during the B6-A window is removed.
    pub fn password_login_allowed(
        db: &Database,
        adminkey_provider: &AdminKeyProvider,
    ) -> AppResult<bool> {
        Ok(!Self::has_active_admin_identity(db, adminkey_provider)?)
    }
}
