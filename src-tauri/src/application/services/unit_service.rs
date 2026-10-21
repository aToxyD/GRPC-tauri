//! Unit Service Module
//!
//! Business logic for unit and associated user management.
//! SQL is delegated to UnitRepository and UserRepository.

use crate::errors::AppError;
use crate::models::{CreateUnitRequest, Unit, UserRole};
use crate::repositories::{DbExecutor, RepositoryProvider, SupplierOrderOwnershipConflict};
use chrono::Utc;
use uuid::Uuid;

use crate::domain::security::PasswordHashPort;

/// Render one nullable ownership value for a domain error message. SQL `NULL`
/// is shown explicitly as `NULL` so both sides of an ownership inconsistency are
/// visible to an administrator.
fn ownership_display(value: &Option<String>) -> &str {
    match value {
        Some(v) => v.as_str(),
        None => "NULL",
    }
}

/// Render an ADR-0064 §4.7 (D4/Q6) supplier-order ownership conflict for a domain
/// error message.
fn format_supplier_order_conflicts(conflicts: &[SupplierOrderOwnershipConflict]) -> String {
    conflicts
        .iter()
        .map(|c| {
            format!(
                "item={} order={} item.unit_id={} order.unit_id={}",
                c.item_id,
                c.order_id,
                ownership_display(&c.item_unit_id),
                ownership_display(&c.order_unit_id),
            )
        })
        .collect::<Vec<_>>()
        .join("; ")
}

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

        // Edit-after-export protection (ADR-0064 §3 D11): refuse edits if any
        // historical identity record exists for this UNIT (status-agnostic).
        use crate::domain::identity::{ports::IdentityStorePort, SubjectType};
        let subject_id = unit_id.parse::<uuid::Uuid>().map_err(|e| {
            crate::errors::AppError::Validation(crate::errors::ValidationError::InvalidFormat {
                field: "unit.id".into(),
                message: format!("invalid UUID: {e}"),
            })
        })?;
        if self
            .executor
            .identity_store()
            .exists_any_by_subject(SubjectType::Unit, &subject_id)?
        {
            return Err(AppError::BusinessLogic(
                crate::errors::BusinessLogicError::OperationNotPermitted {
                    message: format!(
                        "This unit has an existing identity history (status-agnostic); edits are refused after export (subject {})",
                        unit_id
                    ),
                },
            ));
        }

        let now = Utc::now().to_rfc3339();
        unit_repo.update_unit_name(unit_id, &req.name, &now)?;

        unit_repo
            .get_unit(unit_id)?
            .ok_or_else(|| AppError::Internal(format!("Unit not found: {}", unit_id)))
    }

    /// Delete unit with full fail-closed preflight per ADR-0064.
    pub fn delete_unit(&self, unit_id: &str) -> Result<(), AppError> {
        use crate::errors::BusinessLogicError;

        let unit_repo = self.executor.units();
        let user_repo = self.executor.users();

        // P1: resolve target units row
        let unit = unit_repo.get_unit(unit_id)?.ok_or_else(|| {
            AppError::BusinessLogic(BusinessLogicError::ResourceNotFound {
                resource: "unit".to_string(),
                id: unit_id.to_string(),
            })
        })?;

        let unit_code = &unit.code;
        let unit_user_id_from_db = unit.user_id.as_deref();

        // P2: canonical operator resolution (D1.1) — raw lookup (includes deleted = 1).
        // D1.5: duplicate detection is discharged by a counting query, not the single-row helper.
        let canonical_count = user_repo.count_canonical_operators(OPERATOR_USERNAME, unit_code)?;
        if canonical_count > 1 {
            return Err(AppError::BusinessLogic(
                BusinessLogicError::DeleteForbidden {
                    resource: "unit".to_string(),
                    reason: "Multiple canonical operators resolve for this unit code".to_string(),
                },
            ));
        }
        let canonical = user_repo
            .get_user_by_username_raw(OPERATOR_USERNAME, unit_code)?
            .ok_or_else(|| {
                AppError::BusinessLogic(BusinessLogicError::DeleteForbidden {
                    resource: "unit".to_string(),
                    reason: "Canonical operator not found for this unit".to_string(),
                })
            })?;

        // P2 cross-check (D1.2/D1.3): units.user_id must equal the resolved id and must not be NULL.
        if unit_user_id_from_db.is_none() || unit_user_id_from_db != Some(canonical.id.as_str()) {
            return Err(AppError::BusinessLogic(
                BusinessLogicError::DeleteForbidden {
                    resource: "unit".to_string(),
                    reason: "Units.user_id and canonical operator id mismatch".to_string(),
                },
            ));
        }
        let resolved_user_id = canonical.id.clone();

        // P3: D14 gate - audit_log must have no entries for the operator
        let audit_count = unit_repo.count_audit_log_for_user(&resolved_user_id)?;
        if audit_count > 0 {
            return Err(AppError::BusinessLogic(
                BusinessLogicError::DeleteForbidden {
                    resource: "unit".to_string(),
                    reason: "Operator has audit history".to_string(),
                },
            ));
        }

        // P4: aggregate-integrity checks
        let soi_conflicts =
            unit_repo.find_supplier_order_items_with_conflicting_parents(unit_id)?;
        if !soi_conflicts.is_empty() {
            return Err(AppError::BusinessLogic(
                BusinessLogicError::DeleteForbidden {
                    resource: "unit".to_string(),
                    reason: format!(
                        "Target-owned supplier order item(s) hang off a NULL/foreign order: {}",
                        format_supplier_order_conflicts(&soi_conflicts)
                    ),
                },
            ));
        }

        let so_conflicts = unit_repo.find_supplier_orders_with_conflicting_children(unit_id)?;
        if !so_conflicts.is_empty() {
            return Err(AppError::BusinessLogic(
                BusinessLogicError::DeleteForbidden {
                    resource: "unit".to_string(),
                    reason: format!(
                        "Target order(s) own a NULL/foreign child item: {}",
                        format_supplier_order_conflicts(&so_conflicts)
                    ),
                },
            ));
        }

        let alloc_mismatches = unit_repo.find_contract_allocation_unit_mismatches(unit_id)?;
        if !alloc_mismatches.is_empty() {
            return Err(AppError::BusinessLogic(
                BusinessLogicError::DeleteForbidden {
                    resource: "unit".to_string(),
                    reason: format!(
                        "Contract allocation(s) disagree with their parent contract unit: {}",
                        alloc_mismatches.join(", ")
                    ),
                },
            ));
        }

        let soa_mismatches = unit_repo.find_soa_two_sided_mismatches(unit_id)?;
        if !soa_mismatches.is_empty() {
            return Err(AppError::BusinessLogic(
                BusinessLogicError::DeleteForbidden {
                    resource: "unit".to_string(),
                    reason: format!(
                    "Supplier order item allocation(s) cannot prove two-sided target ownership: {}",
                    soa_mismatches.join(", ")
                ),
                },
            ));
        }

        // P5: cross-UNIT reference checks
        let ilm_to_fifo = unit_repo.find_crossunit_ilm_to_fifo(unit_id)?;
        if !ilm_to_fifo.is_empty() {
            return Err(AppError::BusinessLogic(
                BusinessLogicError::DeleteForbidden {
                    resource: "unit".to_string(),
                    reason: format!(
                        "Non-target consumption(s) reference target FIFO layers: {}",
                        ilm_to_fifo.join(", ")
                    ),
                },
            ));
        }

        let ilm_to_mvmt = unit_repo.find_crossunit_ilm_to_movement(unit_id)?;
        if !ilm_to_mvmt.is_empty() {
            return Err(AppError::BusinessLogic(
                BusinessLogicError::DeleteForbidden {
                    resource: "unit".to_string(),
                    reason: format!(
                        "Non-target consumption(s) reference target stock movements: {}",
                        ilm_to_mvmt.join(", ")
                    ),
                },
            ));
        }

        let alloc_to_cp = unit_repo.find_alloc_to_contract_product_conflicts(unit_id)?;
        if !alloc_to_cp.is_empty() {
            return Err(AppError::BusinessLogic(
                BusinessLogicError::DeleteForbidden {
                    resource: "unit".to_string(),
                    reason: format!(
                        "Contract allocation(s) reference target contract products unsafely: {}",
                        alloc_to_cp.join(", ")
                    ),
                },
            ));
        }

        // P6: surviving-report FIFO reference check
        let surviving_fifo_refs = unit_repo.find_surviving_report_fifo_refs(unit_id)?;
        if !surviving_fifo_refs.is_empty() {
            return Err(AppError::BusinessLogic(
                BusinessLogicError::DeleteForbidden {
                    resource: "unit".to_string(),
                    reason: format!(
                        "Surviving report item(s) reference target FIFO layers: {}",
                        surviving_fifo_refs.join(", ")
                    ),
                },
            ));
        }

        // Phase 1: purge
        unit_repo.purge_unit_owned_data(unit_id, &resolved_user_id)?;

        Ok(())
    }

    pub fn get_unit(&self, id: &str) -> Result<Option<Unit>, AppError> {
        self.executor.units().get_unit(id)
    }

    pub fn list_units(
        &self,
        wilaya_code: Option<&str>,
    ) -> Result<Vec<crate::models::unit::UnitView>, AppError> {
        use crate::domain::identity::{ports::IdentityStorePort, SubjectType};

        let units = match wilaya_code {
            Some(wc) => self.executor.units().list_units(wc)?,
            None => self.executor.units().list_all_units()?,
        };
        let mut views = Vec::with_capacity(units.len());
        for unit in units {
            let subject_id = unit.id.parse::<uuid::Uuid>().map_err(|e| {
                crate::errors::AppError::Validation(crate::errors::ValidationError::InvalidFormat {
                    field: "unit.id".into(),
                    message: format!("invalid UUID: {e}"),
                })
            })?;
            let is_exported = self
                .executor
                .identity_store()
                .exists_any_by_subject(SubjectType::Unit, &subject_id)?;
            views.push(crate::models::unit::UnitView {
                id: unit.id,
                code: unit.code,
                name: unit.name,
                wilaya_code: unit.wilaya_code,
                user_id: unit.user_id,
                created_at: unit.created_at,
                is_exported,
            });
        }
        Ok(views)
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
        assert!(port
            .verify_node("0000", CODE, &operator.password_hash)
            .unwrap());
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

    /// ADR-0064 D19: list_units projects is_exported from identity_store history
    #[test]
    fn list_units_projects_is_exported_from_identity_history() {
        use crate::domain::identity::{
            ports::IdentityStorePort, CredentialStatus, IdentityCertificate, SubjectType,
        };
        use uuid::Uuid;

        let db = ConnectionFactory::new_for_test().unwrap();
        let (unit, _) = create(&db, CODE);
        let service = UnitService::new(make_executor(&db), &Argon2PasswordHashProvider);

        // No identity history -> not exported
        let views = service.list_units(Some("WILAYA-1")).unwrap();
        assert_eq!(views.len(), 1);
        assert_eq!(views[0].id, unit.id);
        assert!(!views[0].is_exported);

        // Insert ACTIVE identity history for this UNIT -> exported
        let store = db.executor().identity_store();
        let cert = IdentityCertificate {
            identity_id: Uuid::new_v4(),
            subject_type: SubjectType::Unit,
            subject_id: Uuid::parse_str(&unit.id).unwrap(),
            issuer_identity_id: Some(Uuid::new_v4()),
            credential_id: Uuid::new_v4(),
            generation: 1,
            status: CredentialStatus::Active,
            public_key: vec![1u8; 32],
            algorithm_version: 2,
            not_after: None,
            package_sequence: Some(1),
            signature: Some(crate::domain::identity::test_signature([2u8; 64])),
        };
        store.upsert(&cert, "2026-08-04T00:00:00Z").unwrap();

        let views = service.list_units(Some("WILAYA-1")).unwrap();
        assert_eq!(views.len(), 1);
        assert!(views[0].is_exported);

        // REVOKED also counts
        let mut revoked = cert.clone();
        revoked.identity_id = Uuid::new_v4();
        revoked.status = CredentialStatus::Revoked;
        store.upsert(&revoked, "2026-08-04T00:00:00Z").unwrap();
        let views = service.list_units(Some("WILAYA-1")).unwrap();
        assert!(views[0].is_exported);

        // SUPERSEDED also counts
        let mut superseded = cert.clone();
        superseded.identity_id = Uuid::new_v4();
        superseded.status = CredentialStatus::Superseded;
        store.upsert(&superseded, "2026-08-04T00:00:00Z").unwrap();
        let views = service.list_units(Some("WILAYA-1")).unwrap();
        assert!(views[0].is_exported);

        // EXPIRED also counts
        let mut expired = cert.clone();
        expired.identity_id = Uuid::new_v4();
        expired.status = CredentialStatus::Expired;
        store.upsert(&expired, "2026-08-04T00:00:00Z").unwrap();
        let views = service.list_units(Some("WILAYA-1")).unwrap();
        assert!(views[0].is_exported);

        // Subject scoping - different unit should not be exported
        let (unit2, _) = create(&db, "UNIT78");
        let views = service.list_units(Some("WILAYA-1")).unwrap();
        assert_eq!(views.len(), 2);
        let v2 = views.iter().find(|v| v.id == unit2.id).unwrap();
        assert!(!v2.is_exported);
    }

    /// ADR-0064 §3 D11: updates refused after any identity history exists
    #[test]
    fn update_unit_refused_when_exported_with_any_history() {
        use crate::domain::identity::{
            ports::IdentityStorePort, CredentialStatus, IdentityCertificate, SubjectType,
        };
        use uuid::Uuid;

        let db = ConnectionFactory::new_for_test().unwrap();
        let (unit, _) = create(&db, CODE);
        let service = UnitService::new(make_executor(&db), &Argon2PasswordHashProvider);

        // No history -> name update allowed
        let renamed = service
            .update_unit(
                &unit.id,
                &CreateUnitRequest {
                    code: CODE.to_string(),
                    name: "Renamed".to_string(),
                },
            )
            .unwrap();
        assert_eq!(renamed.name, "Renamed");

        let store = db.executor().identity_store();
        let cert = IdentityCertificate {
            identity_id: Uuid::new_v4(),
            subject_type: SubjectType::Unit,
            subject_id: Uuid::parse_str(&unit.id).unwrap(),
            issuer_identity_id: Some(Uuid::new_v4()),
            credential_id: Uuid::new_v4(),
            generation: 1,
            status: CredentialStatus::Active,
            public_key: vec![1u8; 32],
            algorithm_version: 2,
            not_after: None,
            package_sequence: Some(1),
            signature: Some(crate::domain::identity::test_signature([3u8; 64])),
        };

        // ACTIVE -> refused
        let mut active = cert.clone();
        active.identity_id = Uuid::new_v4();
        active.status = CredentialStatus::Active;
        store.upsert(&active, "2026-08-04T00:00:00Z").unwrap();
        let err = service
            .update_unit(
                &unit.id,
                &CreateUnitRequest {
                    code: CODE.to_string(),
                    name: "ShouldNotChange".to_string(),
                },
            )
            .unwrap_err();
        assert!(matches!(
            err,
            crate::errors::AppError::BusinessLogic(
                crate::errors::BusinessLogicError::OperationNotPermitted { .. }
            )
        ));
        let reloaded = service.get_unit(&unit.id).unwrap().unwrap();
        assert_eq!(reloaded.name, "Renamed");

        // REVOKED -> refused
        let mut revoked = cert.clone();
        revoked.identity_id = Uuid::new_v4();
        revoked.status = CredentialStatus::Revoked;
        store.upsert(&revoked, "2026-08-04T00:00:00Z").unwrap();
        service
            .update_unit(
                &unit.id,
                &CreateUnitRequest {
                    code: CODE.to_string(),
                    name: "StillNo".to_string(),
                },
            )
            .unwrap_err();
        let reloaded = service.get_unit(&unit.id).unwrap().unwrap();
        assert_eq!(reloaded.name, "Renamed");

        // SUPERSEDED -> refused
        let mut superseded = cert.clone();
        superseded.identity_id = Uuid::new_v4();
        superseded.status = CredentialStatus::Superseded;
        store.upsert(&superseded, "2026-08-04T00:00:00Z").unwrap();
        service
            .update_unit(
                &unit.id,
                &CreateUnitRequest {
                    code: CODE.to_string(),
                    name: "StillNo2".to_string(),
                },
            )
            .unwrap_err();
        let reloaded = service.get_unit(&unit.id).unwrap().unwrap();
        assert_eq!(reloaded.name, "Renamed");

        // EXPIRED -> refused
        let mut expired = cert.clone();
        expired.identity_id = Uuid::new_v4();
        expired.status = CredentialStatus::Expired;
        store.upsert(&expired, "2026-08-04T00:00:00Z").unwrap();
        service
            .update_unit(
                &unit.id,
                &CreateUnitRequest {
                    code: CODE.to_string(),
                    name: "StillNo3".to_string(),
                },
            )
            .unwrap_err();
        let reloaded = service.get_unit(&unit.id).unwrap().unwrap();
        assert_eq!(reloaded.name, "Renamed");
    }
}
