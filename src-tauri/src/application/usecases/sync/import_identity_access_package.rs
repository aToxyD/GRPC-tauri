//! Apply decrypted Identity & Access Package (account synchronization) to the
//! local UNIT node.
//!
//! B8 (ADR-0040): kind = `identity_access`. The Wilaya is the single source of
//! truth for the synchronized accounts (fleet-wide `admin` + unit-bound
//! `user`); UNIT nodes apply payloads canonically (rename + canonical upserts).
//! There is no reverse path.
//!
//! Replay protection is owned by the registry via exact `package_id` dedup
//! (SEC-056D/SEC-057): replaying the same package_id is rejected before any
//! mutation; a fresh package_id from the trusted issuer is applied and
//! registered.

use crate::application::services::UserAccountSyncService;
use crate::application::sync::SyncPackage;
use crate::domain::security::PasswordHashPort;
use crate::errors::{AppError, AppResult, BusinessLogicError, ValidationError};
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
    pub package_id: String,
}

pub fn execute(
    executor: DbExecutor<'_>,
    registry: &impl crate::application::sync::ImportedPackageRegistry,
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
    if registry.has_imported(&package_id)? {
        return Err(AppError::BusinessLogic(
            BusinessLogicError::DuplicateSyncPackage {
                package_id: package_id.0.clone(),
            },
        ));
    }

    let outcome =
        UserAccountSyncService::new(executor, password_port).apply(&input.package.payload)?;
    registry.mark_imported(&package_id)?;

    Ok(ImportIdentityAccessPackageOutcome {
        admin_updated: outcome.admin_updated,
        user_updated: outcome.user_updated,
        package_id: package_id.0.clone(),
    })
}
