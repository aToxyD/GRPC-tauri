//! User Account Synchronization Service (B8 — Identity & Access Synchronization).
//!
//! The Wilaya is the single source of truth for the synchronized accounts:
//! - the fleet-wide `admin` (admin derivation domain, valid on every node);
//! - the per-unit `user` (node-bound to the unit code).
//!
//! The Wilaya mutates its own `users` table and exports one
//! [`IdentityAccessPayload`] per unit. UNIT nodes apply payloads canonically
//! (rename the local unit-bound user to `user`, upsert the canonical
//! `admin`/`user` rows). There is no reverse path.

use crate::domain::security::PasswordHashPort;
use crate::errors::{AppError, BusinessLogicError};
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
    pub user_renamed: bool,
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
            .get_user_by_username_raw("admin")?
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
            .get_user_by_username_raw(username)?
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
            .get_user_by_username_raw("admin")?
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

    /// UNIT: apply an [`IdentityAccessPayload`] canonically.
    ///
    /// 1. Rename the local unit-bound user to `user` (only when no `user`
    ///    row exists yet) — resolves the legacy `admin` name collision.
    /// 2. Upsert the canonical `admin` row (fleet-wide hash).
    /// 3. Upsert the canonical `user` row (unit-bound hash).
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

        let local = users.get_user_by_node_id(&payload.unit_code)?;
        if let Some(unit_user) = &local {
            if unit_user.username != "user" && users.get_user_by_username_raw("user")?.is_none() {
                users.update_username(&unit_user.id, "user", &now)?;
                outcome.user_renamed = true;
            }
        }

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

    fn create_unit(db: &Database, code: &str, username: &str) {
        let port = Argon2PasswordHashProvider;
        crate::application::services::UnitService::new(make_executor(db), &port)
            .create_unit(
                &CreateUnitRequest {
                    code: code.to_string(),
                    name: format!("Unit {}", code),
                    username: username.to_string(),
                    password: UNIT_PASSWORD.to_string(),
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

    #[test]
    fn export_carries_fleet_admin_and_unit_user_hashes() {
        let db = ConnectionFactory::new_for_test().unwrap();
        set_fleet_password(&db);
        create_unit(&db, "UNIT-9", "unit9user");

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
        assert!(port
            .verify_node(UNIT_PASSWORD, "UNIT-9", &payload.user_password_hash)
            .expect("verify user"));
    }

    #[test]
    fn export_fails_closed_when_fleet_admin_password_unset() {
        let db = ConnectionFactory::new_for_test().unwrap();
        create_unit(&db, "UNIT-9", "unit9user");

        db.executor()
            .users()
            .change_password(
                &db.executor()
                    .users()
                    .get_user_by_username_raw("admin")
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
        set_fleet_password(&db);
        create_unit(&db, "UNIT-9", "unit9user");

        let port = Argon2PasswordHashProvider;
        UserAccountSyncService::new(make_executor(&db), &port)
            .set_account_status("unit9user", false)
            .expect("disable unit user");

        let payload = UserAccountSyncService::new(make_executor(&db), &port)
            .export("UNIT-9")
            .expect("export");
        assert!(!payload.user_enabled);
        assert!(db
            .executor()
            .users()
            .get_user_by_username("unit9user")
            .unwrap()
            .is_none());
    }

    #[test]
    fn disabled_admin_blocks_export() {
        let db = ConnectionFactory::new_for_test().unwrap();
        set_fleet_password(&db);
        create_unit(&db, "UNIT-9", "unit9user");

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
        create_unit(&db, "UNIT-9", "unit9user");
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
            .get_user_by_username("admin")
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
            .get_user_by_username("user")
            .unwrap()
            .expect("canonical user present");
        assert_eq!(user.role, UserRole::User);
        assert_eq!(user.node_id, "UNIT-9");
        assert!(port
            .verify_node(UNIT_PASSWORD, "UNIT-9", &user.password_hash)
            .expect("verify user"));
    }

    #[test]
    fn apply_renames_legacy_admin_named_unit_user() {
        let db = ConnectionFactory::new_for_test().unwrap();

        // Simulate a pre-sync UNIT node whose local unit-bound user is named
        // "admin" (legacy `.unit` import) — it shadows the seeded admin row
        // (role User, node_id = unit code).
        let port = Argon2PasswordHashProvider;
        let user_hash = port.hash_node(UNIT_PASSWORD, "UNIT-9").expect("user hash");
        db.executor()
            .users()
            .upsert_user(
                "legacy-admin-id",
                "admin",
                &user_hash,
                UserRole::User,
                "UNIT-9",
                "2024-01-01T00:00:00Z",
            )
            .expect("legacy admin-named user upserted");
        let pre_apply_id = db
            .executor()
            .users()
            .get_user_by_node_id("UNIT-9")
            .unwrap()
            .expect("unit-bound user present")
            .id;

        let payload = IdentityAccessPayload {
            unit_code: "UNIT-9".to_string(),
            admin_password_hash: port.hash_admin(FLEET_PASSWORD).expect("admin hash"),
            admin_enabled: true,
            user_password_hash: user_hash,
            user_enabled: true,
        };

        let outcome = UserAccountSyncService::new(make_executor(&db), &port)
            .apply(&payload)
            .expect("apply");
        assert!(
            outcome.user_renamed,
            "legacy admin-named unit user must be renamed"
        );

        let admin = db
            .executor()
            .users()
            .get_user_by_username("admin")
            .unwrap()
            .expect("canonical admin present");
        assert_eq!(admin.role, UserRole::Admin);
        assert_eq!(admin.node_id, "UNIT-9");

        let user = db
            .executor()
            .users()
            .get_user_by_username("user")
            .unwrap()
            .expect("canonical user present");
        assert_eq!(user.role, UserRole::User);
        assert_eq!(user.node_id, "UNIT-9");
        assert_eq!(user.id, pre_apply_id, "rename must keep the row identity");
    }

    #[test]
    fn apply_disabled_accounts_reject_login_and_reapply_reenables() {
        let db = ConnectionFactory::new_for_test().unwrap();
        create_unit(&db, "UNIT-9", "unit9user");

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
                .get_user_by_username("user")
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
                .get_user_by_username("user")
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
