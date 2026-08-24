//! Identity authentication policy (B6-A → B6-B → ADR-0050).
//!
//! RFC 2026-08-04-node-identity-trust / ADR-0038, as amended by ADR-0050
//! (WILAYA Admin Normal Authentication Model).
//!
//! Sole decision source for whether the password login path is permitted for a
//! LOCAL ACCOUNT on this node. The rule is derived from the ACCOUNT CREDENTIAL
//! FACT — does this account carry a usable password hash? — and NEVER from the
//! derived `IdentityBootstrapState` projection (which is a UI / bootstrap
//! concern only, B6-A refinement 1). A derived state may legitimately evolve;
//! this policy is an independent, testable security rule.
//!
//! ADR-0050 supersedes the B6-B permanent gate for normal login: the presence
//! of an ACTIVE ADMIN identity or a `.adminkey` no longer closes the password
//! path. Normal WILAYA Admin login is username + password (the B8 fleet-admin
//! credential); Challenge–Response (`.adminkey`) is retained as the recovery /
//! high-assurance path. The password gate is now an ACCOUNT-LEVEL predicate:
//! an account whose hash is empty (identity-only ADMIN ceremony) has no usable
//! password credential and is routed to Challenge–Response; unknown or
//! soft-deleted accounts fall through to the generic invalid-credentials
//! response (no account enumeration).
//!
//! This preserves Application User Authentication for UNIT local users created
//! from the `.unit` package's `UserExport { username, password_hash, role }`
//! while WILAYA Admin authenticates with the fleet credential (B8).

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
        Ok(Self::admin_credential_state(db, adminkey_provider)? == AdminCredentialState::Usable)
    }

    /// Whether the password login path is permitted for the given local
    /// account.
    ///
    /// ADR-0050 (supersedes B6-B for normal login): the password path is
    /// permitted whenever the LOCAL ACCOUNT carries a usable password
    /// credential — an ACTIVE ADMIN identity or `.adminkey` does NOT close it.
    /// An account with an EMPTY password hash (identity-only ADMIN ceremony)
    /// has no usable password credential and is routed to Challenge–Response.
    /// Unknown and soft-deleted (`deleted = 1`) accounts return `true` so the
    /// command falls through to the generic invalid-credentials response — the
    /// gate must never reveal which usernames exist (no enumeration) nor turn
    /// account absence into a distinguishable `challenge_required` signal.
    /// ADR-0052: lookups are scoped to the local node identity, so a foreign
    /// scope's accounts (e.g. another unit's shadow operator) behave exactly
    /// like unknown accounts.
    pub fn password_login_allowed(
        db: &Database,
        username: &str,
        node_scope: &str,
    ) -> AppResult<bool> {
        let user = db
            .executor()
            .users()
            .get_user_by_username_raw(username, node_scope)?;
        Ok(match user {
            Some(u) => !u.deleted && !u.password_hash.is_empty(),
            None => true,
        })
    }
}
