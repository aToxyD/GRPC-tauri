//! Apply decrypted Identity & Access Package (account synchronization) to the
//! local UNIT node.
//!
//! B8 (ADR-0040): kind = `identity_access`. The Wilaya is the single source of
//! truth for the synchronized accounts (fleet-wide `admin` + unit-bound
//! `user`); UNIT nodes apply payloads canonically (rename + canonical upserts).
//! There is no reverse path.
//!
//! Replay protection is owned exclusively by `run_import_pipeline`
//! (`ImportedPackageRegistry` + Transport Guard) — this usecase deliberately
//! performs no `has_imported`/`mark_imported` bookkeeping.

use crate::application::services::UserAccountSyncService;
use crate::application::sync::SyncPackage;
use crate::domain::security::PasswordHashPort;
use crate::errors::{AppError, AppResult, ValidationError};
use crate::models::IdentityAccessPayload;
use crate::repositories::DbExecutor;

pub const IDENTITY_ACCESS_PACKAGE_KIND: &str = "identity_access";

#[derive(Debug, Clone)]
pub struct ImportIdentityAccessPackageInput {
    pub package: SyncPackage<IdentityAccessPayload>,
    pub imported_by: String,
}

#[derive(Debug, Clone)]
pub struct ImportIdentityAccessPackageOutcome {
    pub admin_updated: bool,
    pub user_updated: bool,
    pub user_renamed: bool,
    pub package_id: String,
}

pub fn execute(
    executor: DbExecutor<'_>,
    _registry: &impl crate::application::sync::ImportedPackageRegistry,
    password_port: &dyn PasswordHashPort,
    input: ImportIdentityAccessPackageInput,
) -> AppResult<ImportIdentityAccessPackageOutcome> {
    if input.package.metadata.source_node_id.trim().is_empty() {
        return Err(AppError::Validation(ValidationError::InvalidFormat {
            field: "source_node_id".into(),
            message: "حزمة حسابات بلا مصدر".into(),
        }));
    }

    let package_id = input.package.metadata.package_id.clone();
    let outcome =
        UserAccountSyncService::new(executor, password_port).apply(&input.package.payload)?;

    Ok(ImportIdentityAccessPackageOutcome {
        admin_updated: outcome.admin_updated,
        user_updated: outcome.user_updated,
        user_renamed: outcome.user_renamed,
        package_id: package_id.0.clone(),
    })
}
