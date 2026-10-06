//! User Account Synchronization Service (B8 — Identity & Access Synchronization).
//!
//! The Wilaya is the single source of truth for the synchronized accounts:
//! - the fleet-wide `admin` (admin derivation domain, valid on every node);
//! - the per-unit `user` (node-bound to the unit code).
//!
//! The Wilaya mutates its own `users` table and exports one
//! [`IdentityAccessPayload`] per unit. UNIT nodes apply payloads canonically
//! (upsert the canonical `admin`/`user` rows). There is no reverse path.
//!
//! ADR-0052: usernames are unique per node scope, so every username lookup in
//! this service is scoped to the local node identity (`"WILAYA"` on a WILAYA
//! node, the local unit code on a UNIT node).

use crate::domain::security::PasswordHashPort;
use crate::errors::{AppError, BusinessLogicError};
use crate::infrastructure::security::node_identity_provider::NodeIdentityProvider as _;
use crate::models::{IdentityAccessPayload, Unit};
use crate::repositories::{DbExecutor, RepositoryProvider};
use chrono::Utc;
use uuid::Uuid;

/// Service orchestrating the synchronized account lifecycle.
pub struct UserAccountSyncService<'a> {
    executor: DbExecutor<'a>,
    password_port: &'a dyn PasswordHashPort,
}

/// Result of applying an [`IdentityAccessPayload`] on a UNIT node.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ApplyIdentityAccessOutcome {
    pub admin_updated: bool,
    pub user_updated: bool,
}

impl<'a> UserAccountSyncService<'a> {
    pub fn new(executor: DbExecutor<'a>, password_port: &'a dyn PasswordHashPort) -> Self {
        Self {
            executor,
            password_port,
        }
    }

    fn not_found(resource: &str, id: &str) -> AppError {
        AppError::BusinessLogic(BusinessLogicError::ResourceNotFound {
            resource: resource.to_string(),
            id: id.to_string(),
        })
    }

    fn not_permitted(message: &str) -> AppError {
        AppError::BusinessLogic(BusinessLogicError::OperationNotPermitted {
            message: message.to_string(),
        })
    }

    /// Local node scope for username lookups (ADR-0052): `"WILAYA"` on a
    /// WILAYA node, the local unit code on a UNIT node.
    fn node_scope(&self) -> String {
        crate::infrastructure::security::SettingsNodeIdentityProvider::new(self.executor)
            .current_node_id()
            .unwrap_or_else(|_| "WILAYA".to_string())
    }

    /// Wilaya: set the fleet-wide `admin` password.
    ///
    /// The hash is derived in the admin domain (identical on every node).
    /// Local modification is forbidden on UNIT nodes by Invariant 13; this
    /// mutation is only reachable through the Wilaya IPC command.
    pub fn set_fleet_admin_password(&self, password: &str) -> Result<(), AppError> {
        crate::domain::validation::validate_change_password(password)?;

        let admin = self
            .executor
            .users()
            .get_user_by_username_raw("admin", &self.node_scope())?
            .ok_or_else(|| Self::not_found("user", "admin"))?;
        let password_hash = self
            .password_port
            .hash_admin(password)
            .map_err(AppError::Internal)?;
        let now = Utc::now().to_rfc3339();
        self.executor
            .users()
            .change_password(&admin.id, &password_hash, &now)?;
        Ok(())
    }

    /// Wilaya: set a unit's `user` password (node-bound to the unit code).
    pub fn set_unit_user_password(&self, unit_code: &str, password: &str) -> Result<(), AppError> {
        crate::domain::validation::validate_change_password(password)?;

        let unit = self
            .executor
            .units()
            .get_unit_by_code(unit_code)?
            .ok_or_else(|| Self::not_found("unit", unit_code))?;
        let user = self.resolve_unit_user(&unit)?;
        let password_hash = self
            .password_port
            .hash_node(password, unit_code)
            .map_err(AppError::Internal)?;
        let now = Utc::now().to_rfc3339();
        self.executor
            .users()
            .change_password(&user.id, &password_hash, &now)?;
        Ok(())
    }

    /// Wilaya: enable / disable an account (soft-delete semantics).
    ///
    /// `enabled = false` maps to `deleted = 1`: the account is rejected at the
    /// login source but the row is preserved and re-enableable.
    pub fn set_account_status(&self, username: &str, enabled: bool) -> Result<(), AppError> {
        let user = self
            .executor
            .users()
            .get_user_by_username_raw(username, &self.node_scope())?
            .ok_or_else(|| Self::not_found("user", username))?;
        let now = Utc::now().to_rfc3339();
        self.executor
            .users()
            .set_deleted(&user.id, !enabled, &now)?;
        Ok(())
    }

    /// Wilaya: build the [`IdentityAccessPayload`] for one unit.
    ///
    /// Fails closed when the fleet `admin` password is not set or the fleet
    /// `admin` is disabled — a package must never distribute an unset or
    /// locked-out fleet credential.
    pub fn export(&self, unit_code: &str) -> Result<IdentityAccessPayload, AppError> {
        let unit = self
            .executor
            .units()
            .get_unit_by_code(unit_code)?
            .ok_or_else(|| Self::not_found("unit", unit_code))?;

        let admin = self
            .executor
            .users()
            .get_user_by_username_raw("admin", &self.node_scope())?
            .ok_or_else(|| Self::not_found("user", "admin"))?;
        if admin.password_hash.is_empty() {
            return Err(Self::not_permitted(
                "كلمة مرور المسؤول العام لم تُضبط بعد؛ حدّثها أولاً",
            ));
        }
        if admin.deleted {
            return Err(Self::not_permitted(
                "حساب المسؤول العام معطّل؛ لا يمكن تصدير حزم حسابات",
            ));
        }

        let user = self.resolve_unit_user(&unit)?;
        Ok(IdentityAccessPayload {
            unit_code: unit.code.clone(),
            admin_password_hash: admin.password_hash,
            admin_enabled: true,
            user_password_hash: user.password_hash,
            user_enabled: !user.deleted,
        })
    }

    /// Wilaya: build the [`AdminAccessPayload`] for the fleet-wide Admin
    /// synchronization package (`kind = "admin_access"`, ADR-0051 — Accepted
    /// 2026-08-22).
    ///
    /// The payload carries ONLY the fleet `admin` credential state — there is
    /// NO unit selector, NO target unit, and NO operator-account material
    /// (structural exclusion, ADR-0051 §4). Fails closed exactly like
    /// [`Self::export`] when the fleet `admin` password is not set or the
    /// account is disabled.
    pub fn export_admin_access(&self) -> Result<crate::models::AdminAccessPayload, AppError> {
        let admin = self
            .executor
            .users()
            .get_user_by_username_raw("admin", &self.node_scope())?
            .ok_or_else(|| Self::not_found("user", "admin"))?;
        if admin.password_hash.is_empty() {
            return Err(Self::not_permitted(
                "كلمة مرور المسؤول العام لم تُضبط بعد؛ حدّثها أولاً",
            ));
        }
        if admin.deleted {
            return Err(Self::not_permitted(
                "حساب المسؤول العام معطّل؛ لا يمكن تصدير حزمة حساب المدير",
            ));
        }

        Ok(crate::models::AdminAccessPayload {
            admin_password_hash: admin.password_hash,
            admin_enabled: true,
        })
    }

    /// UNIT: apply an [`IdentityAccessPayload`] canonically.
    ///
    /// 1. Upsert the canonical `admin` row (fleet-wide hash).
    /// 2. Upsert the canonical `user` row (unit-bound hash).
    ///
    /// Account ownership stays with the unit: `node_id` is the unit code.
    pub fn apply(
        &self,
        payload: &IdentityAccessPayload,
    ) -> Result<ApplyIdentityAccessOutcome, AppError> {
        if payload.admin_password_hash.is_empty() || payload.user_password_hash.is_empty() {
            return Err(Self::not_permitted("حزمة حسابات ناقصة (كلمة مرور فارغة)"));
        }

        let users = self.executor.users();
        let now = Utc::now().to_rfc3339();
        let mut outcome = ApplyIdentityAccessOutcome::default();

        // ADR-0052: no rename step — operator usernames are canonical by
        // construction (`user`, node-scoped). The legacy rename hack is gone.
        let local = users.get_user_by_node_id(&payload.unit_code)?;

        users.upsert_synced_admin(
            &Uuid::new_v4().to_string(),
            &payload.admin_password_hash,
            &payload.unit_code,
            !payload.admin_enabled,
            &now,
        )?;
        outcome.admin_updated = true;

        let user_id = match &local {
            Some(unit_user) => unit_user.id.clone(),
            None => Uuid::new_v4().to_string(),
        };
        users.upsert_synced_user(
            &user_id,
            &payload.user_password_hash,
            &payload.unit_code,
            !payload.user_enabled,
            &now,
        )?;
        outcome.user_updated = true;

        Ok(outcome)
    }

    fn resolve_unit_user(&self, unit: &Unit) -> Result<crate::models::User, AppError> {
        if let Some(user_id) = &unit.user_id {
            if let Some(user) = self.executor.users().get_user_by_id(user_id)? {
                return Ok(user);
            }
        }
        self.executor
            .users()
            .get_user_by_node_id(&unit.code)?
            .ok_or_else(|| Self::not_found("user", &unit.code))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::services::BOOTSTRAP_PASSWORD;
    use crate::db::{ConnectionFactory, Database};
    use crate::domain::security::PasswordHashPort;
    use crate::infrastructure::security::Argon2PasswordHashProvider;
    use crate::models::{CreateUnitRequest, UserRole};
    use crate::repositories::RepositoryProvider;

    const FLEET_PASSWORD: &str = "FleetPass123";
    const UNIT_PASSWORD: &str = "UnitPass123";

    fn make_executor(db: &Database) -> DbExecutor<'_> {
        db.executor()
    }

    fn create_unit(db: &Database, code: &str) {
        let port = Argon2PasswordHashProvider;
        crate::application::services::UnitService::new(make_executor(db), &port)
            .create_unit(
                &CreateUnitRequest {
                    code: code.to_string(),
                    name: format!("Unit {}", code),
                },
                "WILAYA-1",
            )
            .expect("unit created");
    }

    fn set_fleet_password(db: &Database) {
        let port = Argon2PasswordHashProvider;
        UserAccountSyncService::new(make_executor(db), &port)
            .set_fleet_admin_password(FLEET_PASSWORD)
            .expect("fleet password set");
    }

    /// SEC-029: producer-side tests must mirror the production lifecycle —
    /// the `create_unit` IPC command requires `settings.wilaya_code`, which
    /// only `configure_wilaya` sets. Without it the node stays UNCONFIGURED
    /// and the canonical UNIT scope would resolve to the unit code instead
    /// of "WILAYA".
    fn configure_producer_as_wilaya(db: &Database) {
        crate::application::services::SettingsService::new(make_executor(db))
            .configure_wilaya(&crate::models::WilayaNodeConfiguration::new(
                "16".into(),
                "TestWilaya".into(),
            ))
            .expect("producer configured as WILAYA");
    }

    #[test]
    fn export_carries_fleet_admin_and_unit_user_hashes() {
        let db = ConnectionFactory::new_for_test().unwrap();
        configure_producer_as_wilaya(&db);
        set_fleet_password(&db);
        create_unit(&db, "UNIT-9");

        let port = Argon2PasswordHashProvider;
        let payload = UserAccountSyncService::new(make_executor(&db), &port)
            .export("UNIT-9")
            .expect("export");

        assert_eq!(payload.unit_code, "UNIT-9");
        assert!(payload.admin_enabled);
        assert!(payload.user_enabled);
        assert!(port
            .verify_admin(FLEET_PASSWORD, &payload.admin_password_hash)
            .expect("verify admin"));
        // ADR-0063: the created operator's credential is the server-side
        // bootstrap value, not a caller-supplied password.
        assert!(port
            .verify_node(BOOTSTRAP_PASSWORD, "UNIT-9", &payload.user_password_hash)
            .expect("verify user"));
    }

    #[test]
    fn export_fails_closed_when_fleet_admin_password_unset() {
        let db = ConnectionFactory::new_for_test().unwrap();
        configure_producer_as_wilaya(&db);
        create_unit(&db, "UNIT-9");

        db.executor()
            .users()
            .change_password(
                &db.executor()
                    .users()
                    .get_user_by_username_raw("admin", "WILAYA")
                    .unwrap()
                    .unwrap()
                    .id,
                "",
                "2024-01-01T00:00:00Z",
            )
            .unwrap();

        let port = Argon2PasswordHashProvider;
        let err = UserAccountSyncService::new(make_executor(&db), &port)
            .export("UNIT-9")
            .expect_err("export must fail closed");
        assert!(matches!(
            err,
            AppError::BusinessLogic(BusinessLogicError::OperationNotPermitted { .. })
        ));
    }

    #[test]
    fn export_reflects_disabled_unit_user() {
        let db = ConnectionFactory::new_for_test().unwrap();
        configure_producer_as_wilaya(&db);
        set_fleet_password(&db);
        create_unit(&db, "UNIT-9");

        let port = Argon2PasswordHashProvider;
        // ADR-0052: shadow operators are node-scoped (`user`, unit code);
        // disabling goes through the row id, exactly like the authorized
        // account-status mutation path.
        let operator_id = db
            .executor()
            .users()
            .get_user_by_node_id("UNIT-9")
            .unwrap()
            .expect("operator present")
            .id;
        db.executor()
            .users()
            .set_deleted(&operator_id, false, "2024-01-01T00:00:00Z")
            .unwrap();

        let payload = UserAccountSyncService::new(make_executor(&db), &port)
            .export("UNIT-9")
            .expect("export");
        assert!(payload.user_enabled);

        db.executor()
            .users()
            .set_deleted(&operator_id, true, "2024-01-02T00:00:00Z")
            .unwrap();
        let payload = UserAccountSyncService::new(make_executor(&db), &port)
            .export("UNIT-9")
            .expect("export");
        assert!(!payload.user_enabled);
        assert!(db
            .executor()
            .users()
            .get_user_by_username("user", "UNIT-9")
            .unwrap()
            .is_none());
    }

    /// ADR-0052: foreign-scope usernames are indistinguishable from unknown
    /// ones — the WILAYA scope cannot address a UNIT operator row.
    #[test]
    fn set_account_status_is_scope_bounded() {
        let db = ConnectionFactory::new_for_test().unwrap();
        configure_producer_as_wilaya(&db);
        set_fleet_password(&db);
        create_unit(&db, "UNIT-9");

        let port = Argon2PasswordHashProvider;
        let err = UserAccountSyncService::new(make_executor(&db), &port)
            .set_account_status("user", false)
            .expect_err("foreign-scope operator must not be addressable");
        assert!(matches!(
            err,
            AppError::BusinessLogic(BusinessLogicError::ResourceNotFound { .. })
        ));
    }

    #[test]
    fn disabled_admin_blocks_export() {
        let db = ConnectionFactory::new_for_test().unwrap();
        configure_producer_as_wilaya(&db);
        set_fleet_password(&db);
        create_unit(&db, "UNIT-9");

        let port = Argon2PasswordHashProvider;
        UserAccountSyncService::new(make_executor(&db), &port)
            .set_account_status("admin", false)
            .expect("disable admin");

        let err = UserAccountSyncService::new(make_executor(&db), &port)
            .export("UNIT-9")
            .expect_err("export must fail closed");
        assert!(matches!(
            err,
            AppError::BusinessLogic(BusinessLogicError::OperationNotPermitted { .. })
        ));
    }

    #[test]
    fn apply_creates_canonical_admin_and_user_on_unit() {
        let db = ConnectionFactory::new_for_test().unwrap();
        configure_producer_as_wilaya(&db);
        create_unit(&db, "UNIT-9");
        set_fleet_password(&db);

        let port = Argon2PasswordHashProvider;
        let payload = UserAccountSyncService::new(make_executor(&db), &port)
            .export("UNIT-9")
            .expect("export");

        let outcome = UserAccountSyncService::new(make_executor(&db), &port)
            .apply(&payload)
            .expect("apply");
        assert!(outcome.admin_updated);
        assert!(outcome.user_updated);

        let admin = db
            .executor()
            .users()
            .get_user_by_username("admin", "UNIT-9")
            .unwrap()
            .expect("admin present");
        assert_eq!(admin.role, UserRole::Admin);
        assert_eq!(admin.node_id, "UNIT-9");
        assert!(port
            .verify_admin(FLEET_PASSWORD, &admin.password_hash)
            .expect("verify admin"));

        let user = db
            .executor()
            .users()
            .get_user_by_username("user", "UNIT-9")
            .unwrap()
            .expect("canonical user present");
        assert_eq!(user.role, UserRole::User);
        assert_eq!(user.node_id, "UNIT-9");
        assert!(port
            .verify_node(BOOTSTRAP_PASSWORD, "UNIT-9", &user.password_hash)
            .expect("verify user"));
    }

    /// ADR-0052: apply() preserves the existing canonical operator row —
    /// no rename step exists anywhere in the pipeline.
    #[test]
    fn apply_preserves_canonical_operator_row_identity() {
        let db = ConnectionFactory::new_for_test().unwrap();
        create_unit(&db, "UNIT-9");

        let port = Argon2PasswordHashProvider;
        let original_id = db
            .executor()
            .users()
            .get_user_by_username("user", "UNIT-9")
            .unwrap()
            .expect("canonical operator present")
            .id;

        let payload = IdentityAccessPayload {
            unit_code: "UNIT-9".to_string(),
            admin_password_hash: port.hash_admin(FLEET_PASSWORD).expect("admin hash"),
            admin_enabled: true,
            user_password_hash: port.hash_node(UNIT_PASSWORD, "UNIT-9").expect("user hash"),
            user_enabled: true,
        };

        let outcome = UserAccountSyncService::new(make_executor(&db), &port)
            .apply(&payload)
            .expect("apply");
        assert!(outcome.admin_updated);
        assert!(outcome.user_updated);

        let user = db
            .executor()
            .users()
            .get_user_by_username("user", "UNIT-9")
            .unwrap()
            .expect("canonical user present");
        assert_eq!(user.id, original_id, "operator row identity is preserved");
        assert_eq!(
            db.executor()
                .users()
                .get_user_by_username_raw("user", "UNIT-9")
                .unwrap()
                .expect("raw row present")
                .username,
            "user"
        );
    }

    #[test]
    fn apply_disabled_accounts_reject_login_and_reapply_reenables() {
        let db = ConnectionFactory::new_for_test().unwrap();
        configure_producer_as_wilaya(&db);
        create_unit(&db, "UNIT-9");

        let port = Argon2PasswordHashProvider;
        let payload = UserAccountSyncService::new(make_executor(&db), &port)
            .export("UNIT-9")
            .expect("export");
        let mut disabled = payload.clone();
        disabled.user_enabled = false;

        UserAccountSyncService::new(make_executor(&db), &port)
            .apply(&disabled)
            .expect("apply disabled");
        assert!(
            db.executor()
                .users()
                .get_user_by_username("user", "UNIT-9")
                .unwrap()
                .is_none(),
            "disabled unit user must be rejected"
        );

        UserAccountSyncService::new(make_executor(&db), &port)
            .apply(&payload)
            .expect("re-apply enabled");
        assert!(
            db.executor()
                .users()
                .get_user_by_username("user", "UNIT-9")
                .unwrap()
                .is_some(),
            "re-enabled unit user must authenticate at source"
        );
    }

    #[test]
    fn apply_rejects_payload_with_empty_hash() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let port = Argon2PasswordHashProvider;
        let mut payload = IdentityAccessPayload {
            unit_code: "UNIT-9".to_string(),
            admin_password_hash: "hash".to_string(),
            admin_enabled: true,
            user_password_hash: String::new(),
            user_enabled: true,
        };
        let err = UserAccountSyncService::new(make_executor(&db), &port)
            .apply(&payload)
            .expect_err("empty user hash must be rejected");
        assert!(matches!(
            err,
            AppError::BusinessLogic(BusinessLogicError::OperationNotPermitted { .. })
        ));
        payload.user_password_hash = "hash".to_string();
        payload.admin_password_hash = String::new();
        let err = UserAccountSyncService::new(make_executor(&db), &port)
            .apply(&payload)
            .expect_err("empty admin hash must be rejected");
        assert!(matches!(
            err,
            AppError::BusinessLogic(BusinessLogicError::OperationNotPermitted { .. })
        ));
    }
}
