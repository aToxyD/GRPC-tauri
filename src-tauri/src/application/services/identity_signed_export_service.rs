//! Producer-side Ed25519 (`signature_version = 2`) sync package export.
//!
//! RFC 2026-08-04-node-identity-trust §3.4.1 / §3.10 / B6-B (Commit ④b).
//!
//! Single entry point for V2 production exports (products, daily_report,
//! monthly_summary, stock_movements, admin_access) and for `.unit`
//! bootstrap artifacts (ADR-0044). The service:
//!
//! - resolves the local node identity via [`NodeIdentityResolver`] — the
//!   authoritative, fail-closed resolver (R5). There is deliberately NO HMAC
//!   fallback: an unprovisioned node cannot emit a V2 package;
//! - builds + signs + writes the encrypted package through `PackageBuilder`
//!   with the resolved Ed25519 signer;
//! - is transport-sequence-free (SEC-056D/SEC-057): no producer allocator,
//!   no per-issuer/per-target sequence. Each package carries a fresh
//!   `package_id`; replay protection on the receiving node is
//!   `package_id`-exact only.
//!
//! `.unit` bootstrap packages share the identical sequence-free shape and do
//! not route through any ledger. Legacy `identity_access` issuance remains
//! dead at the command boundary (ADR-0051 D1 cutover); no export path for it
//! exists in this service.

use std::path::Path;

use serde::Serialize;

use crate::application::services::NodeIdentityResolver;
use crate::application::sync::{
    PackageExportMode, PackageId, SyncPackage, SyncPackageMetadata, SYNC_PACKAGE_SCHEMA_VERSION,
};
use crate::db::Database;
use crate::domain::identity::{SubjectType, SIGNATURE_VERSION_ED25519};
use crate::errors::{AppError, AppResult, BusinessLogicError};
use crate::infrastructure::identity::NodeKeyStore;
use crate::infrastructure::security::file_encryption::AgeFileEncryptionProvider;
use crate::infrastructure::sync::packages::signing::Ed25519PackageSigner;
use crate::infrastructure::sync::{PackageBuilder, SerdeJsonSyncPackageSerializer};
use chrono::Utc;
use uuid::Uuid;

/// Producer-side V2 export orchestration.
pub struct IdentitySignedExportService<'a> {
    db: &'a Database,
    node_key_store: &'a NodeKeyStore,
}

impl<'a> IdentitySignedExportService<'a> {
    pub fn new(db: &'a Database, node_key_store: &'a NodeKeyStore) -> Self {
        Self { db, node_key_store }
    }

    /// Export `dataset` as an Ed25519-signed V2 sync package to `target_path`.
    ///
    /// Fail-closed: `None` from the resolver (no node key, no ACTIVE
    /// certificate, R5 mismatch, non-Ed25519 algorithm) is an error — there is
    /// no HMAC fallback path for production sync exports.
    ///
    /// `export_mode` / `target_node_id` are authenticated Contract Catalog
    /// replay metadata (SEC-087 Phase 2) carried inside the signed envelope.
    /// `None` for both on every non-Contract-Catalog producer.
    #[allow(clippy::too_many_arguments)]
    pub fn export_v2_package<T: Serialize>(
        &self,
        dataset: T,
        source_node_id: &str,
        kind: &str,
        export_mode: Option<PackageExportMode>,
        target_node_id: Option<&str>,
        target_path: &Path,
        node_type: SubjectType,
        crypto_port: &AgeFileEncryptionProvider,
    ) -> AppResult<()> {
        let resolved = NodeIdentityResolver::resolve_local_signer(self.db, self.node_key_store, node_type)?
            .ok_or_else(|| {
                AppError::BusinessLogic(BusinessLogicError::OperationNotPermitted {
                    message: format!(
                        "لا يمكن تصدير حزمة V2: العقدة المحلية غير مزوّدة كهوية {node_type} (مفتاح عقدة أو شهادة نشطة ناقصة)"
                    ),
                })
            })?;

        let identity_id = resolved.certificate.identity_id;
        let signer = Ed25519PackageSigner::from_provider(resolved.signer);
        self.build_and_write(
            dataset,
            source_node_id,
            kind,
            export_mode,
            target_node_id,
            identity_id,
            signer,
            target_path,
            crypto_port,
        )?;

        log::info!(
            target: "grpc::sync",
            "identity signed export success: kind={} issuer={} path={}",
            kind,
            identity_id,
            target_path.display()
        );

        Ok(())
    }

    /// Export the fleet Admin synchronization package (ADR-0051, kind
    /// `admin_access`) as an Ed25519-signed V2 sync package to `target_path`.
    ///
    /// The payload carries ONLY `{admin_password_hash, admin_enabled}`
    /// (structural isolation from any operator-account material). The identity
    /// of the issuing node (`issuer_identity_id`) binds authenticity; no
    /// transport sequence is assigned (SEC-056D/SEC-057).
    ///
    /// Fail-closed resolver behavior identical to [`Self::export_v2_package`].
    pub fn export_v2_admin_access_package(
        &self,
        dataset: crate::models::AdminAccessPayload,
        source_node_id: &str,
        _target_unit_code: &str,
        target_path: &Path,
        node_type: SubjectType,
        crypto_port: &AgeFileEncryptionProvider,
    ) -> AppResult<()> {
        let resolved = NodeIdentityResolver::resolve_local_signer(self.db, self.node_key_store, node_type)?
            .ok_or_else(|| {
                AppError::BusinessLogic(BusinessLogicError::OperationNotPermitted {
                    message: format!(
                        "لا يمكن تصدير حزمة V2: العقدة المحلية غير مزوّدة كهوية {node_type} (مفتاح عقدة أو شهادة نشطة ناقصة)"
                    ),
                })
            })?;

        let identity_id = resolved.certificate.identity_id;
        let signer = Ed25519PackageSigner::from_provider(resolved.signer);
        self.build_and_write(
            dataset,
            source_node_id,
            crate::application::usecases::sync::import_admin_access_package::ADMIN_ACCESS_PACKAGE_KIND,
            None,
            None,
            identity_id,
            signer,
            target_path,
            crypto_port,
        )?;

        log::info!(
            target: "grpc::sync",
            "admin access export success: kind={} issuer={} path={}",
            crate::application::usecases::sync::import_admin_access_package::ADMIN_ACCESS_PACKAGE_KIND,
            identity_id,
            target_path.display()
        );

        Ok(())
    }

    /// Build + sign + write the encrypted V2 package file (shared by all
    /// export paths). Produces NO ledger effect.
    #[allow(clippy::too_many_arguments)]
    fn build_and_write<T: Serialize>(
        &self,
        dataset: T,
        source_node_id: &str,
        kind: &str,
        export_mode: Option<PackageExportMode>,
        target_node_id: Option<&str>,
        identity_id: Uuid,
        signer: Ed25519PackageSigner,
        target_path: &Path,
        crypto_port: &AgeFileEncryptionProvider,
    ) -> AppResult<()> {
        let package = SyncPackage {
            metadata: SyncPackageMetadata {
                schema_version: SYNC_PACKAGE_SCHEMA_VERSION,
                created_at: Utc::now(),
                source_node_id: source_node_id.to_string(),
                issuer_identity_id: Some(identity_id),
                package_id: PackageId(Uuid::new_v4().to_string()),
                signature_version: Some(SIGNATURE_VERSION_ED25519),
                signing_key_id: Some(signer.public_key_hex()),
                integrity_hash: None,
                signature: None,
                export_mode,
                target_node_id: target_node_id.map(str::to_string),
            },
            payload: dataset,
        };

        PackageBuilder::new()
            .build_encrypted_stream_path(
                &package,
                &SerdeJsonSyncPackageSerializer,
                &signer,
                crypto_port,
                target_path,
            )
            .map_err(|e| {
                log::warn!(
                    target: "grpc::sync",
                    "identity signed export failed: kind={} issuer={} error={}",
                    kind,
                    identity_id,
                    e
                );
                e
            })?;

        log::info!(
            target: "grpc::sync",
            "identity signed package written: kind={} issuer={} path={}",
            kind,
            identity_id,
            target_path.display()
        );

        Ok(())
    }

    /// Export a one-time bootstrap artifact (`.unit` V2, ADR-0044 A44-08).
    ///
    /// Same Ed25519 identity-bound signing as `export_v2_package`, with NO
    /// transport sequence and NO ledger allocation (SEC-056D/SEC-057). The
    /// receiving UNIT accepts it only anchor-first, against the locally
    /// installed ACTIVE WILAYA anchor (Root → WILAYA → Ed25519).
    ///
    /// Fails closed exactly like `export_v2_package` when the local node is
    /// not provisioned with the requested identity.
    pub fn export_v2_bootstrap_package<T: Serialize>(
        &self,
        dataset: T,
        source_node_id: &str,
        target_path: &Path,
        node_type: SubjectType,
        crypto_port: &AgeFileEncryptionProvider,
    ) -> AppResult<()> {
        let resolved = NodeIdentityResolver::resolve_local_signer(self.db, self.node_key_store, node_type)?
            .ok_or_else(|| {
                AppError::BusinessLogic(BusinessLogicError::OperationNotPermitted {
                    message: format!(
                        "لا يمكن تصدير حزمة العقدة V2: العقدة المحلية غير مزوّدة كهوية {node_type} (مفتاح عقدة أو شهادة نشطة ناقصة)"
                    ),
                })
            })?;

        let identity_id = resolved.certificate.identity_id;
        let signer = Ed25519PackageSigner::from_provider(resolved.signer);
        let package = SyncPackage {
            metadata: SyncPackageMetadata {
                schema_version: SYNC_PACKAGE_SCHEMA_VERSION,
                created_at: Utc::now(),
                source_node_id: source_node_id.to_string(),
                issuer_identity_id: Some(identity_id),
                package_id: PackageId(Uuid::new_v4().to_string()),
                signature_version: Some(SIGNATURE_VERSION_ED25519),
                signing_key_id: Some(signer.public_key_hex()),
                integrity_hash: None,
                signature: None,
                export_mode: None,
                target_node_id: None,
            },
            payload: dataset,
        };

        PackageBuilder::new()
            .build_encrypted_stream_path(
                &package,
                &SerdeJsonSyncPackageSerializer,
                &signer,
                crypto_port,
                target_path,
            )
            .map_err(|e| {
                log::warn!(
                    target: "grpc::sync",
                    "bootstrap package export failed: kind=unit kind_issuer={} error={}",
                    identity_id,
                    e
                );
                e
            })?;

        log::info!(
            target: "grpc::sync",
            "bootstrap package export success: kind=unit issuer={} path={}",
            identity_id,
            target_path.display()
        );

        Ok(())
    }
}

impl crate::architecture::Service for IdentitySignedExportService<'_> {}
