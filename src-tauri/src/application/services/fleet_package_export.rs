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
use crate::application::sync::PackageExportMode;
use crate::application::usecases::exports::export_contract_catalog_dataset;
use crate::application::usecases::exports::export_products_dataset;
use crate::application::usecases::exports::types::{
    ExportContractCatalogInput, ExportContractCatalogMode, ExportProductsInput,
};
use crate::application::usecases::sync::import_contract_catalog_package::CONTRACT_CATALOG_PACKAGE_KIND;
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
        |exporter, _target, path| {
            exporter.export_v2_package(
                dataset.clone(),
                source_node_id,
                PRODUCTS_PACKAGE_KIND,
                None,
                None,
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

/// Export a UNIT-scoped ContractCatalog artifact to EXACTLY the requested
/// unit targets (ADR-0059 §9). Each target receives an INDEPENDENT dataset — selected, validated and signed for that unit alone — never a clone of a
/// fleet-wide artifact:
///
/// 1. fail-closed resolution of every requested target code (any unknown /
///    ineligible target aborts the whole batch BEFORE any emission);
/// 2. one independent `UnitDistribution` dataset per resolved target
///    (contracts restricted to the target's internal unit id, allocations /
///    exceptions via that contract chain, suppliers/links restricted to the
///    target's relationships, global tax policies);
/// 3. only then emits per-target artifacts with authenticated
///    `(UnitDistribution, target code)` metadata.
///
/// Partial-failure semantics mirror trust rotation: the first failing target
/// aborts (fail loud); artifacts already written earlier remain standalone-
/// valid packages. An empty `targets` slice is rejected — batch intent must
/// be materialized, it cannot be silently inferred from the whole fleet.
pub fn export_contract_catalog_unit_distribution(
    db: &mut Database,
    node_key_store: &NodeKeyStore,
    crypto_port: &AgeFileEncryptionProvider,
    source_node_id: &str,
    subject_type: SubjectType,
    requested_path: &Path,
    targets: &[String],
) -> AppResult<FleetExportOutcome> {
    if targets.is_empty() {
        return Err(crate::errors::AppError::Validation(
            crate::errors::ValidationError::InvalidFormat {
                field: "targets".into(),
                message:
                    "توزيع كتالوج العقود بالوحدة يتطلب قائمة وحدات مستهدفة — أدخل واحدة على الأقل"
                        .into(),
            },
        ));
    }

    // Phase 1 — fail-closed resolution + dataset construction for EVERY
    // target. All executor-backed work MUST precede any emission (the exporter
    // holds the database for signing/sequence state).
    let resolved = targets
        .iter()
        .map(|code| {
            let dataset = export_contract_catalog_dataset::execute(
                db.executor(),
                ExportContractCatalogInput {
                    mode: ExportContractCatalogMode::UnitDistribution {
                        target_unit_code: code.clone(),
                    },
                },
            )?;
            Ok::<_, crate::errors::AppError>((code.clone(), dataset))
        })
        .collect::<crate::errors::AppResult<Vec<_>>>()?;

    // Phase 2 — emission (no executor-backed reads beyond the exporter).
    let mut artifact_paths = Vec::with_capacity(resolved.len());
    let mut record_count = 0usize;
    {
        let exporter = IdentitySignedExportService::new(db, node_key_store);
        for (code, dataset) in &resolved {
            let artifact_path = transport_target::derive_per_target_artifact_path(
                requested_path,
                code,
                targets.len(),
            );
            exporter.export_v2_package(
                dataset.clone(),
                source_node_id,
                CONTRACT_CATALOG_PACKAGE_KIND,
                Some(PackageExportMode::UnitDistribution),
                Some(code),
                &artifact_path,
                subject_type,
                crypto_port,
            )?;
            record_count += dataset.contracts.len();
            artifact_paths.push(artifact_path);
        }
    }

    Ok(FleetExportOutcome {
        targets: resolved.iter().map(|(code, _)| code.clone()).collect(),
        artifact_paths,
        record_count,
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
    let payload =
        super::UserAccountSyncService::new(db.executor(), password_port).export_admin_access()?;
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
    F: Fn(&IdentitySignedExportService<'_>, &str, &Path) -> AppResult<()>,
{
    let exporter = IdentitySignedExportService::new(db, node_key_store);
    let mut artifact_paths = Vec::with_capacity(targets.len());
    for target in targets {
        let artifact_path = transport_target::derive_per_target_artifact_path(
            requested_path,
            target,
            targets.len(),
        );
        emit(&exporter, target, &artifact_path)?;
        artifact_paths.push(artifact_path);
    }
    Ok(artifact_paths)
}
