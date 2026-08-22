//! Apply a decrypted Admin-Only B8 account synchronization package
//! (`kind = "admin_access"`, ADR-0051 — Accepted 2026-08-22) to the local
//! UNIT node.
//!
//! ADR-0051 §5 (Account Ownership Invariant): `.unit` provisioning owns the
//! UNIT-local operator account; `admin_access` owns ONLY the canonical
//! `admin` account. This usecase is the structural enforcement of that
//! boundary — its apply path reaches ONLY [`crate::repositories::UsersRepository::upsert_synced_admin`]
//! and deliberately contains NO dependency on UNIT-user synchronization code
//! (`upsert_synced_user`, `update_username`, unit-user password mutation).
//! The operator row is therefore byte-for-byte unreachable from this kind,
//! on both acceptance and rejection branches.
//!
//! The payload ([`AdminAccessPayload`]) carries exactly
//! `{admin_password_hash, admin_enabled}` and rejects unknown fields at
//! deserialization, so no target-unit or operator-account material can enter.
//! The synchronized username is structurally canonical (`admin`, hard-coded
//! by the repository upsert) and is never transported.
//!
//! Replay protection is owned exclusively by `run_import_pipeline`
//! (`ImportedPackageRegistry` + Transport Guard) — this usecase performs no
//! `has_imported`/`mark_imported` bookkeeping.

use crate::application::sync::SyncPackage;
use crate::errors::{AppError, AppResult, BusinessLogicError};
use crate::models::AdminAccessPayload;
use crate::repositories::{DbExecutor, RepositoryProvider};
use uuid::Uuid;

pub const ADMIN_ACCESS_PACKAGE_KIND: &str = "admin_access";

#[derive(Debug, Clone)]
pub struct ImportAdminAccessPackageInput {
    pub package: SyncPackage<AdminAccessPayload>,
    /// The importing UNIT's own trusted local unit code (persisted settings,
    /// re-read inside the import transaction). Used solely as the canonical
    /// `admin` row's `node_id` stamp — identical end-state semantics to the
    /// certified fleet-admin upsert. It NEVER travels in the payload.
    pub local_unit_code: String,
}

#[derive(Debug, Clone)]
pub struct ImportAdminAccessPackageOutcome {
    pub admin_updated: bool,
    pub package_id: String,
}

pub fn execute(
    executor: DbExecutor<'_>,
    input: ImportAdminAccessPackageInput,
) -> AppResult<ImportAdminAccessPackageOutcome> {
    if input.package.metadata.source_node_id.trim().is_empty() {
        return Err(AppError::Validation(
            crate::errors::ValidationError::InvalidFormat {
                field: "source_node_id".into(),
                message: "حزمة حساب المدير بلا مصدر".into(),
            },
        ));
    }
    if input.local_unit_code.trim().is_empty() {
        // Fail closed BEFORE any mutation: without a trusted local unit code
        // the canonical admin row cannot be stamped correctly.
        return Err(AppError::BusinessLogic(
            BusinessLogicError::OperationNotPermitted {
                message: "لا يمكن تطبيق حزمة حساب المدير: رمز الوحدة المحلية غير معروف".into(),
            },
        ));
    }
    if input.package.payload.admin_password_hash.is_empty() {
        return Err(AppError::BusinessLogic(
            BusinessLogicError::OperationNotPermitted {
                message: "حزمة حساب المدير ناقصة (كلمة مرور فارغة)".into(),
            },
        ));
    }

    let now = chrono::Utc::now().to_rfc3339();
    executor.users().upsert_synced_admin(
        &Uuid::new_v4().to_string(),
        &input.package.payload.admin_password_hash,
        input.local_unit_code.trim(),
        !input.package.payload.admin_enabled,
        &now,
    )?;

    Ok(ImportAdminAccessPackageOutcome {
        admin_updated: true,
        package_id: input.package.metadata.package_id.0.clone(),
    })
}
