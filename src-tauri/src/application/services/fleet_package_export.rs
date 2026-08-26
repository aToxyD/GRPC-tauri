//! Fleet-level WILAYA package exports (SEC-033).
//!
//! A WILAYA operator expresses fleet-level intent ("export the products
//! catalog" / "synchronize the admin account"); THIS module — never the
//! renderer — enumerates the authoritative target set from local
//! `units` rows and emits one signed, encrypted V2 artifact per target.
//!
//! Per-target transport isolation (ADR-0053) is preserved untouched: each
//! artifact draws its sequence from the canonical `(issuer_identity_id,
//! target_node_id)` stream via [`IdentitySignedExportService`], with
//! advance-on-success / no-burn semantics. Target enumeration, fail-closed
//! validation, and per-target artifact naming live in
//! [`super::transport_target`] (single source of truth, shared with trust
//! rotation).
//!
//! Partial-failure semantics mirror trust rotation: the first failing target
//! aborts the export (fail loud); artifacts already written for earlier
//! targets remain — each is a standalone-valid, signed package whose sequence
//! was committed on successful write only.

use super::transport_target;
use super::IdentitySignedExportService;
use crate::application::usecases::exports::export_products_dataset;
use crate::application::usecases::exports::types::ExportProductsInput;
use crate::application::usecases::sync::import_products_package::PRODUCTS_PACKAGE_KIND;
use crate::db::Database;
use crate::domain::identity::SubjectType;
use crate::domain::security::PasswordHashPort;
use crate::errors::AppResult;
use crate::infrastructure::identity::NodeKeyStore;
use crate::infrastructure::security::file_encryption::AgeFileEncryptionProvider;
use std::path::{Path, PathBuf};

/// Outcome of a fleet-level export: the authoritative targets served and the
/// concrete artifact paths written (one per target, order-stable).
/// `record_count` is the per-artifact payload record count (products rows /
/// 1 admin account), preserving [`crate::models::PackageExportResult`]
/// semantics.
#[derive(Debug, Clone)]
pub struct FleetExportOutcome {
    pub targets: Vec<String>,
    pub artifact_paths: Vec<PathBuf>,
    pub record_count: usize,
}

/// Export the products catalog to EVERY authoritative UNIT target.
///
/// The payload is identical across artifacts; only the transport metadata
/// (per-target stream sequence + issuer binding) differs.
pub fn export_products_fleet(
    db: &mut Database,
    node_key_store: &NodeKeyStore,
    crypto_port: &AgeFileEncryptionProvider,
    source_node_id: &str,
    subject_type: SubjectType,
    requested_path: &Path,
) -> AppResult<FleetExportOutcome> {
    let dataset = export_products_dataset::execute(db.executor(), ExportProductsInput)?;
    let targets = transport_target::resolve_fleet_unit_targets(db.executor())?;
    emit_per_target(
        db,
        node_key_store,
        |exporter, target, path| {
            exporter.export_v2_package(
                dataset.clone(),
                source_node_id,
                PRODUCTS_PACKAGE_KIND,
                target,
                path,
                subject_type,
                crypto_port,
            )
        },
        &targets,
        requested_path,
    )
    .map(|artifact_paths| FleetExportOutcome {
        targets,
        artifact_paths,
        record_count: dataset.product_rows.len(),
    })
}

/// Export the fleet Admin synchronization payload (`admin_access`, ADR-0051)
/// to EVERY authoritative UNIT target. Fails closed while the fleet admin
/// password is unset or the account is disabled (payload build precedes any
/// emission).
pub fn export_admin_access_fleet(
    db: &mut Database,
    node_key_store: &NodeKeyStore,
    password_port: &dyn PasswordHashPort,
    crypto_port: &AgeFileEncryptionProvider,
    source_node_id: &str,
    subject_type: SubjectType,
    requested_path: &Path,
) -> AppResult<FleetExportOutcome> {
    let payload = super::UserAccountSyncService::new(db.executor(), password_port)
        .export_admin_access()?;
    let targets = transport_target::resolve_fleet_unit_targets(db.executor())?;
    emit_per_target(
        db,
        node_key_store,
        |exporter, target, path| {
            exporter.export_v2_admin_access_package(
                payload.clone(),
                source_node_id,
                target,
                path,
                subject_type,
                crypto_port,
            )
        },
        &targets,
        requested_path,
    )
    .map(|artifact_paths| FleetExportOutcome {
        targets,
        artifact_paths,
        // The payload carries exactly one account: the fleet `admin`.
        record_count: 1,
    })
}

/// Shared emission loop: derive each artifact path, write, and collect.
///
/// All executor-backed work (payload build, target enumeration) MUST precede
/// this call — the exporter holds the database for signing/sequence state.
fn emit_per_target<F>(
    db: &mut Database,
    node_key_store: &NodeKeyStore,
    emit: F,
    targets: &[String],
    requested_path: &Path,
) -> AppResult<Vec<PathBuf>>
where
    F: Fn(&IdentitySignedExportService<'_>, &str, &Path) -> AppResult<u64>,
{
    let exporter = IdentitySignedExportService::new(db, node_key_store);
    let mut artifact_paths = Vec::with_capacity(targets.len());
    for target in targets {
        let artifact_path =
            transport_target::derive_per_target_artifact_path(requested_path, target, targets.len());
        let _sequence = emit(&exporter, target, &artifact_path)?;
        artifact_paths.push(artifact_path);
    }
    Ok(artifact_paths)
}
