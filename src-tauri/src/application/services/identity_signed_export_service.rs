//! Producer-side Ed25519 (`signature_version = 2`) sync package export.
//!
//! RFC 2026-08-04-node-identity-trust §3.4.1 / §3.10 / B6-B (Commit ④b) /
//! ADR-0053 (Unified Per-Target Transport Sequence).
//!
//! Single entry point for V2 production exports (products, daily_report,
//! monthly_summary, stock_movements, admin_access) and for `.unit`
//! bootstrap artifacts (ADR-0044). The service:
//!
//! - resolves the local node identity via [`NodeIdentityResolver`] — the
//!   authoritative, fail-closed resolver (R5). There is deliberately NO HMAC
//!   fallback: an unprovisioned node cannot emit a V2 package;
//! - allocates the transport sequence from ONE canonical producer stream per
//!   `(issuer_identity_id, target_node_id)` across ALL pipeline kinds
//!   (ADR-0053), with the target resolved authoritatively by
//!   `crate::application::services::transport_target` BEFORE allocation;
//! - builds + signs + writes the encrypted package through `PackageBuilder`
//!   with the resolved Ed25519 signer;
//! - advances the producer ledger ONLY on success (`commit`), so a failed
//!   export burns no sequence and a retry reuses the same number.
//!
//! The only ledger-advancing component is the pending-sequence token — the
//! service never touches the ledger directly. `.unit` bootstrap packages
//! remain fixed-sequence-1 artifacts (RFC §7 Bootstrap exception, ADR-0044
//! A44-08) and do NOT route through any ledger. Legacy `identity_access`
//! issuance remains dead at the command boundary (ADR-0051 D1 cutover); no
//! export path for it exists in this service.

use std::path::Path;

use serde::Serialize;

use crate::application::services::NodeIdentityResolver;
use crate::application::sync::{
    PackageId, SyncPackage, SyncPackageMetadata, SYNC_PACKAGE_SCHEMA_VERSION,
};
use crate::db::Database;
use crate::domain::identity::{SubjectType, SIGNATURE_VERSION_ED25519};
use crate::errors::{AppError, AppResult, BusinessLogicError};
use crate::infrastructure::identity::NodeKeyStore;
use crate::infrastructure::security::file_encryption::AgeFileEncryptionProvider;
use crate::infrastructure::sync::packages::signing::Ed25519PackageSigner;
use crate::infrastructure::sync::{PackageBuilder, SerdeJsonSyncPackageSerializer};
use crate::repositories::RepositoryProvider;
use chrono::Utc;
use uuid::Uuid;

/// Producer-side V2 export orchestration. Not thread-safe by design — the
/// single-writer SQLite model serializes all export allocation anyway.
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
    /// `target_node_id` is the canonical ADR-0053 allocation key — resolved
    /// authoritatively by the caller (`transport_target`: `units.code` for
    /// UNIT recipients, `settings.wilaya_code` for WILAYA recipients) and
    /// re-validated fail-closed here (empty/blank ⇒ error before any
    /// allocation).
    ///
    /// Returns the issued transport sequence (1-based, monotonic per
    /// `(issuer_identity_id, target_node_id)` stream across all package
    /// kinds). The producer ledger is advanced ONLY after the package file
    /// has been successfully built and written.
    ///
    /// Fail-closed: `None` from the resolver (no node key, no ACTIVE
    /// certificate, R5 mismatch, non-Ed25519 algorithm) is an error — there is
    /// no HMAC fallback path for production sync exports.
    #[allow(clippy::too_many_arguments)]
    pub fn export_v2_package<T: Serialize>(
        &self,
        dataset: T,
        source_node_id: &str,
        kind: &str,
        target_node_id: &str,
        target_path: &Path,
        node_type: SubjectType,
        crypto_port: &AgeFileEncryptionProvider,
    ) -> AppResult<u64> {
        let resolved = NodeIdentityResolver::resolve_local_signer(self.db, self.node_key_store, node_type)?
            .ok_or_else(|| {
                AppError::BusinessLogic(BusinessLogicError::OperationNotPermitted {
                    message: format!(
                        "لا يمكن تصدير حزمة V2: العقدة المحلية غير مزوّدة كهوية {node_type} (مفتاح عقدة أو شهادة نشطة ناقصة)"
                    ),
                })
            })?;

        let identity_id = resolved.certificate.identity_id;
        let pending = self
            .db
            .executor()
            .transport_export_sequence_state()
            .begin_export(&identity_id.to_string(), target_node_id)?;

        let sequence = pending.value();
        let signer = Ed25519PackageSigner::from_provider(resolved.signer);
        self.build_and_write(
            dataset,
            source_node_id,
            kind,
            sequence,
            identity_id,
            signer,
            target_path,
            crypto_port,
        )?;

        // Advance-on-success: commit the allocated sequence only after the
        // package file was produced. A failed build above dropped the token and
        // the ledger is untouched.
        pending.commit()?;

        log::info!(
            target: "grpc::sync",
            "identity signed export success: kind={} issuer={} target={} sequence={} path={}",
            kind,
            identity_id,
            target_node_id,
            sequence,
            target_path.display()
        );

        Ok(sequence)
    }

    /// Export the fleet Admin synchronization package (ADR-0051, kind
    /// `admin_access`) as an Ed25519-signed V2 sync package to `target_path`.
    ///
    /// ADR-0053 supersedes the dedicated issuer-only stream of ADR-0051 §7:
    /// allocation joins the canonical per-target transport stream with
    /// `target_node_id = target_unit_code` — an authoritative `units.code`
    /// validated by the caller via
    /// [`resolve_unit_transport_target`](super::transport_target) and
    /// re-validated fail-closed here. Each target UNIT therefore receives its
    /// own contiguous stream from this issuer, consistent with its local
    /// kind-blind consumer ledger.
    ///
    /// The payload carries ONLY `{admin_password_hash, admin_enabled}`
    /// (structural isolation from any operator-account material).
    ///
    /// Advance-on-success and fail-closed resolver behavior are identical to
    /// [`Self::export_v2_package`].
    pub fn export_v2_admin_access_package(
        &self,
        dataset: crate::models::AdminAccessPayload,
        source_node_id: &str,
        target_unit_code: &str,
        target_path: &Path,
        node_type: SubjectType,
        crypto_port: &AgeFileEncryptionProvider,
    ) -> AppResult<u64> {
        let resolved = NodeIdentityResolver::resolve_local_signer(self.db, self.node_key_store, node_type)?
            .ok_or_else(|| {
                AppError::BusinessLogic(BusinessLogicError::OperationNotPermitted {
                    message: format!(
                        "لا يمكن تصدير حزمة V2: العقدة المحلية غير مزوّدة كهوية {node_type} (مفتاح عقدة أو شهادة نشطة ناقصة)"
                    ),
                })
            })?;

        let identity_id = resolved.certificate.identity_id;
        let pending = self
            .db
            .executor()
            .transport_export_sequence_state()
            .begin_export(&identity_id.to_string(), target_unit_code)?;

        let sequence = pending.value();
        let signer = Ed25519PackageSigner::from_provider(resolved.signer);
        self.build_and_write(
            dataset,
            source_node_id,
            crate::application::usecases::sync::import_admin_access_package::ADMIN_ACCESS_PACKAGE_KIND,
            sequence,
            identity_id,
            signer,
            target_path,
            crypto_port,
        )?;

        // Advance-on-success, scoped to the (issuer, target) stream only.
        pending.commit()?;

        log::info!(
            target: "grpc::sync",
            "admin access export success: kind={} issuer={} target={} sequence={} path={}",
            crate::application::usecases::sync::import_admin_access_package::ADMIN_ACCESS_PACKAGE_KIND,
            identity_id,
            target_unit_code,
            sequence,
            target_path.display()
        );

        Ok(sequence)
    }

    /// Build + sign + write the encrypted V2 package file for an allocated
    /// sequence (shared by the global-ledger and per-target-ledger export
    /// paths). Produces NO ledger effect — allocation commit is the caller's
    /// responsibility (advance-on-success).
    #[allow(clippy::too_many_arguments)]
    fn build_and_write<T: Serialize>(
        &self,
        dataset: T,
        source_node_id: &str,
        kind: &str,
        sequence: u64,
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
                package_sequence: Some(sequence),
                issuer_identity_id: Some(identity_id),
                package_id: PackageId(Uuid::new_v4().to_string()),
                signature_version: Some(SIGNATURE_VERSION_ED25519),
                signing_key_id: Some(signer.public_key_hex()),
                integrity_hash: None,
                signature: None,
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
                    "identity signed export failed (no sequence burned): kind={} issuer={} error={}",
                    kind,
                    identity_id,
                    e
                );
                e
            })?;

        log::info!(
            target: "grpc::sync",
            "identity signed package written: kind={} issuer={} sequence={} path={}",
            kind,
            identity_id,
            sequence,
            target_path.display()
        );

        Ok(())
    }

    /// Export a one-time bootstrap artifact (`.unit` V2, ADR-0044 A44-08).
    ///
    /// Same Ed25519 identity-bound signing as `export_v2_package`, but with a
    /// FIXED `package_sequence = 1` and NO ledger allocation: the `.unit` is
    /// outside the Transport Guard ordering domain (RFC §3.10), so the first
    /// `identity_access` import still opens the consumer ledger at sequence 1
    /// (A45-06). The receiving UNIT accepts it only anchor-first, against the
    /// locally installed ACTIVE WILAYA anchor (Root → WILAYA → Ed25519).
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
    ) -> AppResult<u64> {
        let resolved = NodeIdentityResolver::resolve_local_signer(self.db, self.node_key_store, node_type)?
            .ok_or_else(|| {
                AppError::BusinessLogic(BusinessLogicError::OperationNotPermitted {
                    message: format!(
                        "لا يمكن تصدير حزمة العقدة V2: العقدة المحلية غير مزوّدة كهوية {node_type} (مفتاح عقدة أو شهادة نشطة ناقصة)"
                    ),
                })
            })?;

        let identity_id = resolved.certificate.identity_id;
        let sequence = 1u64;
        let signer = Ed25519PackageSigner::from_provider(resolved.signer);
        let package = SyncPackage {
            metadata: SyncPackageMetadata {
                schema_version: SYNC_PACKAGE_SCHEMA_VERSION,
                created_at: Utc::now(),
                source_node_id: source_node_id.to_string(),
                package_sequence: Some(sequence),
                issuer_identity_id: Some(identity_id),
                package_id: PackageId(Uuid::new_v4().to_string()),
                signature_version: Some(SIGNATURE_VERSION_ED25519),
                signing_key_id: Some(signer.public_key_hex()),
                integrity_hash: None,
                signature: None,
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
            "bootstrap package export success: kind=unit issuer={} sequence=1 path={}",
            identity_id,
            target_path.display()
        );

        Ok(sequence)
    }
}

impl crate::architecture::Service for IdentitySignedExportService<'_> {}
