//! Unit Service Module
//!
//! Business logic for unit and associated user management.
//! SQL is delegated to UnitRepository and UserRepository.

use crate::errors::AppError;
use crate::models::{CreateUnitRequest, Unit, UserRole};
use crate::repositories::{DbExecutor, RepositoryProvider};
use chrono::Utc;
use uuid::Uuid;

use crate::domain::security::PasswordHashPort;

/// Canonical UNIT operator username (ADR-0052). Every UNIT operator account
/// is named `user`; uniqueness is node-scoped to the unit code, so operators
/// of different units coexist. The value is derived server-side and is never
/// caller-supplied.
pub const OPERATOR_USERNAME: &str = "user";

/// Bootstrap credential for a newly created canonical UNIT operator
/// (ADR-0063 §4). Temporary by construction: the row is created with
/// `must_change_password = true`, so this value must be rotated before normal
/// application use.
///
/// It is a **bootstrap-only** value, held in this single server-side constant.
/// It is never a caller-supplied input, never a reset value (ADR-0063 D6), and
/// never a recovery value (ADR-0063 D17). It is hashed through the ordinary
/// node-bound construction `hash_node(BOOTSTRAP_PASSWORD, <unit code>)`, so the
/// resulting credential format and node binding are identical to any other
/// operator credential — no new derivation domain, KDF parameter, or hash
/// format.
///
/// The value intentionally fails all three composition rules of
/// `validate_change_password`. That is exactly why it is minted on the
/// provisioning path without that policy check rather than by relaxing the
/// policy: every other password-mutating entry point keeps calling
/// `validate_change_password` unchanged.
pub const BOOTSTRAP_PASSWORD: &str = "0000";

/// Service for unit management business logic
pub struct UnitService<'a> {
    executor: DbExecutor<'a>,
    password_port: &'a dyn PasswordHashPort,
}

impl<'a> UnitService<'a> {
    /// Create a new UnitService with the given executor
    pub fn new(executor: DbExecutor<'a>, password_port: &'a dyn PasswordHashPort) -> Self {
        Self {
            executor,
            password_port,
        }
    }

    /// Create a unit with its associated canonical operator account.
    ///
    /// ADR-0052: the operator username is the canonical [`OPERATOR_USERNAME`]
    /// (`user`), persisted with `node_id = code` — one operator per unit,
    /// many across the fleet, independent node-bound password hashes.
    ///
    /// ADR-0063 §3/§4: the credential is **server-authoritative**. The caller
    /// supplies only `code` and `name`; the bootstrap credential
    /// ([`BOOTSTRAP_PASSWORD`]) is minted here, node-bound to the unit code, and
    /// the row starts in the forced-change state. No caller-supplied password
    /// is accepted, forwarded, or derived.
    ///
    /// The caller is responsible for wrapping in a transaction:
    /// `db.with_transaction(|tx| UnitService::new(tx).create_unit(req, wilaya_code))`
    pub fn create_unit(
        &self,
        req: &CreateUnitRequest,
        wilaya_code: &str,
    ) -> Result<(Unit, String), AppError> {
        let user_repo = self.executor.users();
        let unit_repo = self.executor.units();

        // Business logic: use unit code as node_id for node-bound password hashing
        let node_id = &req.code;

        // Hash the bootstrap credential with node binding. This is the single
        // provisioning site that mints the bootstrap value without applying
        // `validate_change_password` (ADR-0063 §4).
        let password_hash = self
            .password_port
            .hash_node(BOOTSTRAP_PASSWORD, node_id)
            .map_err(crate::errors::AppError::Internal)?;

        let user_id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        user_repo.upsert_user(
            &user_id,
            OPERATOR_USERNAME,
            &password_hash,
            UserRole::User,
            node_id,
            true,
            &now,
        )?;

        let unit_id = Uuid::new_v4().to_string();
        let unit_created_at_raw = Utc::now().to_rfc3339();
        unit_repo.insert_unit(&unit_id, req, wilaya_code, &user_id, &unit_created_at_raw)?;
        let created_at = crate::errors::parse_datetime_rfc3339(&unit_created_at_raw)
            .map_err(|e| crate::errors::AppError::Internal(e.to_string()))?;

        let unit = Unit {
            id: unit_id,
            code: req.code.clone(),
            name: req.name.clone(),
            wilaya_code: wilaya_code.to_string(),
            user_id: Some(user_id.clone()),
            created_at,
        };

        Ok((unit, user_id))
    }

    /// Update a unit's editable information.
    ///
    /// Business rules:
    /// - `name` remains editable.
    /// - `code` is immutable (ADR-0063 D8): a request carrying a different
    ///   code is rejected by the domain validation layer, never written, and
    ///   never compensated for by rotating the operator credential. The
    ///   compensating "rotate the hash under the new code" design is rejected
    ///   by the owner and is not implemented.
    /// - The operator username is never touched (ADR-0052), and this flow has
    ///   no password capability at all: the canonical credential is changed
    ///   only through the dedicated credential lifecycle (ADR-0063 §6/§7).
    pub fn update_unit(&self, unit_id: &str, req: &CreateUnitRequest) -> Result<Unit, AppError> {
        let unit_repo = self.executor.units();

        let existing = unit_repo
            .get_unit(unit_id)?
            .ok_or_else(|| AppError::Internal(format!("Unit not found: {}", unit_id)))?;

        crate::domain::validation::validate_unit_code_immutable(&existing.code, &req.code)?;

        let now = Utc::now().to_rfc3339();
        unit_repo.update_unit_name(unit_id, &req.name, &now)?;

        unit_repo
            .get_unit(unit_id)?
            .ok_or_else(|| AppError::Internal(format!("Unit not found: {}", unit_id)))
    }

    /// Delete unit and its associated user atomically.
    pub fn delete_unit(&self, unit_id: &str) -> Result<(), AppError> {
        let user_repo = self.executor.users();
        let unit_repo = self.executor.units();

        if let Some(user_id) = unit_repo.delete_unit(unit_id)? {
            user_repo.delete_user(&user_id)?;
        }
        Ok(())
    }

    pub fn get_unit(&self, id: &str) -> Result<Option<Unit>, AppError> {
        self.executor.units().get_unit(id)
    }

    pub fn list_units(&self, wilaya_code: &str) -> Result<Vec<Unit>, AppError> {
        self.executor.units().list_units(wilaya_code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::ConnectionFactory;
    use crate::domain::validation::validate_change_password;
    use crate::infrastructure::security::Argon2PasswordHashProvider;
    use crate::repositories::DbExecutor;

    const CODE: &str = "UNIT77";

    fn make_executor(db: &crate::db::Database) -> DbExecutor<'_> {
        db.executor()
    }

    fn create(db: &crate::db::Database, code: &str) -> (Unit, String) {
        UnitService::new(make_executor(db), &Argon2PasswordHashProvider)
            .create_unit(
                &CreateUnitRequest {
                    code: code.to_string(),
                    name: format!("Unit {}", code),
                },
                "WILAYA-1",
            )
            .expect("unit created")
    }

    /// ADR-0063 §3/§4: creating a UNIT with only `code` + `name` mints exactly
    /// one canonical `user` operator, bound to the created unit's node identity,
    /// holding the bootstrap credential in the forced-change state.
    #[test]
    fn create_unit_mints_one_canonical_operator_in_forced_state() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let (unit, user_id) = create(&db, CODE);

        assert_eq!(unit.code, CODE);
        assert_eq!(unit.user_id.as_deref(), Some(user_id.as_str()));

        let operators: Vec<_> = db
            .executor()
            .users()
            .list_users()
            .unwrap()
            .into_iter()
            .filter(|u| u.node_id == CODE)
            .collect();

        assert_eq!(
            operators.len(),
            1,
            "exactly one canonical operator must exist for the unit node"
        );
        let operator = &operators[0];
        assert_eq!(operator.id, user_id);
        assert_eq!(operator.username, OPERATOR_USERNAME);
        assert_eq!(operator.role, UserRole::User);
        assert!(operator.must_change_password);
    }

    /// ADR-0063 §4: the bootstrap value is node-bound like any other operator
    /// credential — same construction, same format, no new derivation domain.
    #[test]
    fn bootstrap_credential_verifies_under_the_existing_mechanism() {
        let db = ConnectionFactory::new_for_test().unwrap();
        create(&db, CODE);

        let operator = db
            .executor()
            .users()
            .get_user_by_username(OPERATOR_USERNAME, CODE)
            .unwrap()
            .expect("canonical operator present");

        let port = Argon2PasswordHashProvider;
        assert!(port.verify_node("0000", CODE, &operator.password_hash).unwrap());
        // Node binding preserved: the same value does not authenticate elsewhere.
        assert!(!port
            .verify_node("0000", "UNIT78", &operator.password_hash)
            .unwrap());
        assert!(!port
            .verify_node("other", CODE, &operator.password_hash)
            .unwrap());
    }

    /// ADR-0063 §4: the normal password policy is unchanged. `0000` violates
    /// all three composition rules, which is exactly why it is minted on the
    /// provisioning path instead of by relaxing the policy.
    #[test]
    fn bootstrap_value_is_not_accepted_by_the_normal_policy() {
        assert_eq!(BOOTSTRAP_PASSWORD, "0000");
        assert!(
            validate_change_password(BOOTSTRAP_PASSWORD).is_err(),
            "the bootstrap value must remain invalid for every non-bootstrap path"
        );
        assert!(validate_change_password("UnitPass123").is_ok());
    }

    /// ADR-0063 D8: `code` is immutable; `name` stays editable.
    #[test]
    fn update_unit_rejects_code_change_and_keeps_name_editable() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let (unit, _) = create(&db, CODE);
        let service = UnitService::new(make_executor(&db), &Argon2PasswordHashProvider);

        // Re-sending the same code with a new name is the supported edit.
        let renamed = service
            .update_unit(
                &unit.id,
                &CreateUnitRequest {
                    code: CODE.to_string(),
                    name: "Renamed Unit".to_string(),
                },
            )
            .expect("rename is allowed");
        assert_eq!(renamed.name, "Renamed Unit");
        assert_eq!(renamed.code, CODE);

        // Any attempt to change the code is refused, and nothing is written.
        let err = service
            .update_unit(
                &unit.id,
                &CreateUnitRequest {
                    code: "UNIT88".to_string(),
                    name: "Renamed Unit".to_string(),
                },
            )
            .expect_err("code change must be refused");
        assert!(
            err.to_string().contains("رمز الوحدة"),
            "expected the immutability refusal, got {err:?}"
        );

        let reloaded = service.get_unit(&unit.id).unwrap().expect("unit present");
        assert_eq!(reloaded.code, CODE);
        assert_eq!(reloaded.name, "Renamed Unit");
    }
}
