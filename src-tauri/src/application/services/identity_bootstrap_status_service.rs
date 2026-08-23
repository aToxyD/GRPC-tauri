//! Identity bootstrap status projection (B5).
//!
//! RFC 2026-08-04-node-identity-trust §3.6 / §3.12 / ADR-0038.
//!
//! `compute` is a PURE projection: the returned `IdentityBootstrapState` is
//! derived from the Identity Store, the node key store, the `.adminkey`, and
//! the linked admin user's credential state. It is NEVER persisted and holds no
//! internal state — the same inputs always produce the same state (P1).
//!
//! Node-type branching (RFC §3.12 D2): UNIT nodes walk the UNIT chain
//! (`Uninitialized → UnitWaitingForCertificate → UnitActive`); everything else
//! — including the unconfigured seed, whose raw `node_type` is `UNCONFIGURED`
//! and must NOT be mistaken for a UNIT node — walks the WILAYA chain. The
//! WILAYA chain is therefore the default.

use crate::application::services::identity_provisioning_service::BOOTSTRAP_ADMIN_USERNAME;
use crate::db::Database;
use crate::domain::identity::{IdentityBootstrapState, IdentityStorePort, SubjectType};
use crate::errors::AppResult;
use crate::infrastructure::identity::{AdminKeyProvider, NodeKeyStore};
use crate::repositories::RepositoryProvider;

/// Computes the derived bootstrap state for the local node.
pub struct IdentityBootstrapStatusService;

impl IdentityBootstrapStatusService {
    pub fn compute(
        db: &Database,
        node_key_store: &NodeKeyStore,
        adminkey_provider: &AdminKeyProvider,
    ) -> AppResult<IdentityBootstrapState> {
        let node_type = db.executor().settings().get_node_type()?;
        if node_type.eq_ignore_ascii_case("UNIT") {
            return Self::compute_unit_chain(db, node_key_store);
        }
        Self::compute_wilaya_chain(db, node_key_store, adminkey_provider)
    }

    /// WILAYA chain: `Uninitialized → WaitingForRootCertificate → WilayaActive
    /// → AdminProvisioned → Ready` (RFC §3.6). The DEFAULT chain: an
    /// unconfigured node (`UNCONFIGURED` seed) is not a UNIT node.
    fn compute_wilaya_chain(
        db: &Database,
        node_key_store: &NodeKeyStore,
        adminkey_provider: &AdminKeyProvider,
    ) -> AppResult<IdentityBootstrapState> {
        let store = db.executor().identity_store();
        let wilaya = store.get_active_by_subject_type(SubjectType::Wilaya)?;
        let admin_cert = store.get_active_by_subject_type(SubjectType::Admin)?;

        if wilaya.is_some() && admin_cert.is_some() {
            use crate::infrastructure::security::node_identity_provider::NodeIdentityProvider as _;
            let node_scope = crate::infrastructure::security::SettingsNodeIdentityProvider::new(
                db.executor(),
            )
            .current_node_id()
            .unwrap_or_else(|_| "WILAYA".to_string());
            let admin_user = db
                .executor()
                .users()
                .get_user_by_username(BOOTSTRAP_ADMIN_USERNAME, &node_scope)?;
            // Ready vs AdminProvisioned: Ready requires the operator to be able
            // to authenticate — an identity-only (empty hash) admin user AND the
            // portable `.adminkey` present on the node. A legacy password hash,
            // a missing user row, or a missing `.adminkey` keeps the state at
            // AdminProvisioned (additive window / inconsistent → fail-closed).
            return Ok(match admin_user {
                Some(user) if user.password_hash.is_empty() && adminkey_provider.exists() => {
                    IdentityBootstrapState::Ready
                }
                _ => IdentityBootstrapState::AdminProvisioned,
            });
        }
        if wilaya.is_some() {
            return Ok(IdentityBootstrapState::WilayaActive);
        }
        if node_key_store.exists() {
            return Ok(IdentityBootstrapState::WaitingForRootCertificate);
        }
        Ok(IdentityBootstrapState::Uninitialized)
    }

    /// UNIT chain: `Uninitialized → UnitWaitingForCertificate → UnitActive`
    /// (RFC §3.12 / B6-A). The UNIT node holds no ADMIN, so the WILAYA admin
    /// projection is irrelevant here.
    fn compute_unit_chain(
        db: &Database,
        node_key_store: &NodeKeyStore,
    ) -> AppResult<IdentityBootstrapState> {
        let store = db.executor().identity_store();
        if store
            .get_active_by_subject_type(SubjectType::Unit)?
            .is_some()
        {
            return Ok(IdentityBootstrapState::UnitActive);
        }
        if node_key_store.exists() {
            return Ok(IdentityBootstrapState::UnitWaitingForCertificate);
        }
        Ok(IdentityBootstrapState::Uninitialized)
    }
}
