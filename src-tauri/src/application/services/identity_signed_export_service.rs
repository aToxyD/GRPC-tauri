//! Producer-side Ed25519 (`signature_version = 2`) sync package export.
//!
//! RFC 2026-08-04-node-identity-trust §3.4.1 / §3.10 / B6-B (Commit ④b).
//!
//! Single entry point for V2 production exports (products, daily_report,
//! monthly_summary, stock_movements). The service:
//!
//! - resolves the local node identity via [`NodeIdentityResolver`] — the
//!   authoritative, fail-closed resolver (R5). There is deliberately NO HMAC
//!   fallback: an unprovisioned node cannot emit a V2 package;
//! - allocates a per-issuer transport sequence with `begin_export` and stamps
//!   it into the metadata (`identity_id`-keyed, never `credential_id`);
//! - builds + signs + writes the encrypted package through `PackageBuilder`
//!   with the resolved Ed25519 signer;
//! - advances the producer ledger ONLY on success (`commit`), so a failed
//!   export burns no sequence and a retry reuses the same number.
//!
//! The only ledger-advancing component is `PendingIssuedSequence` — the service
//! never touches the ledger directly. `.unit` bootstrap packages remain HMAC
//! (RFC §7 Bootstrap exception) and do NOT route through this service.

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
    /// Returns the issued transport sequence (1-based, monotonic per local
    /// node identity). The producer ledger is advanced ONLY after the package
    /// file has been successfully built and written.
    ///
    /// Fail-closed: `None` from the resolver (no node key, no ACTIVE
    /// certificate, R5 mismatch, non-Ed25519 algorithm) is an error — there is
    /// no HMAC fallback path for production sync exports.
    pub fn export_v2_package<T: Serialize>(
        &self,
        dataset: T,
        source_node_id: &str,
        kind: &str,
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
            .sync_issuer_sequence_state()
            .begin_export(&identity_id.to_string())?;

        let sequence = pending.value();
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
                    "identity signed export failed (no sequence burned): kind={} issuer={} error={}",
                    kind,
                    identity_id,
                    e
                );
                e
            })?;

        // Advance-on-success: commit the allocated sequence only after the
        // package file was produced. A failed build above dropped the token and
        // the ledger is untouched.
        pending.commit()?;

        log::info!(
            target: "grpc::sync",
            "identity signed export success: kind={} issuer={} sequence={} path={}",
            kind,
            identity_id,
            sequence,
            target_path.display()
        );

        Ok(sequence)
    }
}

impl crate::architecture::Service for IdentitySignedExportService<'_> {}
