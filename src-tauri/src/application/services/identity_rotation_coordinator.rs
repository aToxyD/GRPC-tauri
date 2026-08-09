//! Identity rotation orchestration (B7, behavioral cutover).
//!
//! RFC 2026-08-04-node-identity-trust §3.3 / §3.4.2 / §3.12 / ADR-0038.
//!
//! Wires the pure planner/verifier [`IdentityRotationService`] into persistence
//! and filesystem side effects. `IdentityRotationCoordinator` is the ONLY
//! application-layer owner of the rotation flow; IPC commands stay thin.
//!
//! Ordering guarantees:
//! - `begin` stages the new node secret via `NodeKeyStore::write_pending` and
//!   NEVER touches the active key — the active key stays authoritative until
//!   finalize promotes it (so a rotation trust package is signed with the OLD
//!   key, see `finalize_wilaya`).
//! - `finalize_*` verifies fail-closed (R5, issuer/Root signature, identity
//!   stability, subject binding, ACTIVE, Credential Guard) BEFORE any side
//!   effect. Replay is a ZERO-write idempotent no-op.
//! - WILAYA finalize writes the rotation trust package (kind `trust`) BEFORE
//!   the secret promotion; on promotion/install failure the old key is
//!   restored so the node never carries an R5-broken (new key + old cert) pair.
//! - `sign_unit_rotation` signs the UNIT rotation CSR and records it in the
//!   WILAYA-side Identity Store as **Issuer Local State** — NOT a distribution
//!   channel (Trust Package remains the sole inter-node channel, §3.4.4). The
//!   WILAYA-side credential guard is best-effort: a rollback/zero-generation
//!   conflict skips the local registration (with a warning) without blocking
//!   the signing; the final security decision stays at the UNIT's
//!   `finalize_unit_rotation` (fail-closed).

use std::path::Path;

use chrono::Utc;

use crate::application::services::{
    FinalizeVerdict, IdentityProvisioningService, IdentityRotationService,
    IdentitySignedExportService, RotationOperation,
};
use crate::application::sync_integrity::credential_guard::{CredentialGuard, CredentialVerdict};
use crate::application::usecases::sync::import_trust_package::{
    TrustPackagePayload, TRUST_PACKAGE_KIND,
};
use crate::db::Database;
use crate::domain::identity::{
    CredentialStatus, IdentityCertificate, IdentityStorePort, SubjectType,
};
use crate::errors::{AppError, AppResult, BusinessLogicError};
use crate::infrastructure::identity::{resolve_root_public_key, NodeKeyStore};
use crate::infrastructure::security::file_encryption::AgeFileEncryptionProvider;
use crate::infrastructure::security::Ed25519SigningProvider;
use crate::infrastructure::sync::resolve_export_source_node_id;
use crate::models::Settings;
use crate::repositories::RepositoryProvider;

/// Outcome of a rotation `finalize_*`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum RotationFinalizeOutcome {
    /// New authoritative credential state — the rotation was completed.
    Completed {
        certificate: IdentityCertificate,
        operation: RotationOperation,
    },
    /// Identical re-presentation of the stored ACTIVE certificate — ZERO writes.
    AlreadyCompleted { certificate: IdentityCertificate },
}

/// Result of `sign_unit_rotation`: the signed certificate plus the operation
/// derived from the WILAYA-side store (for audit classification).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SignedUnitRotation {
    pub certificate: IdentityCertificate,
    pub operation: RotationOperation,
}

/// Application-layer rotation orchestrator. Not thread-safe by design — the
/// single-writer SQLite model serializes all mutations anyway.
pub struct IdentityRotationCoordinator<'a> {
    db: &'a mut Database,
    node_key_store: &'a NodeKeyStore,
}

impl<'a> IdentityRotationCoordinator<'a> {
    pub fn new(db: &'a mut Database, node_key_store: &'a NodeKeyStore) -> Self {
        Self { db, node_key_store }
    }

    /// Begin a rotation: stage a fresh node key and build the unsigned CSR.
    ///
    /// Fail-closed: no ACTIVE identity → error; plan failure discards the
    /// staged key so nothing is left dangling.
    pub fn begin(
        &mut self,
        subject_type: SubjectType,
        operation: RotationOperation,
    ) -> AppResult<crate::application::services::RotationPlan> {
        let stored = self
            .db
            .executor()
            .identity_store()
            .get_active_by_subject_type(subject_type)?
            .ok_or_else(|| {
                Self::permitted(format!("No ACTIVE {subject_type} identity to rotate"))
            })?;

        let (secret, _) = Self::generate_keypair();
        self.node_key_store.write_pending(&secret)?;
        match IdentityRotationService::plan(operation, &stored, &secret) {
            Ok(plan) => Ok(plan),
            Err(e) => {
                let _ = self.node_key_store.discard_pending();
                Err(e)
            }
        }
    }

    /// Finalize the WILAYA rotation with the Root-signed certificate.
    ///
    /// On Accept: writes the rotation Trust Package (signed with the OLD key,
    /// still ACTIVE) to `rotation_package_path`, promotes the staged secret,
    /// then installs the new ACTIVE WILAYA certificate. Any failure after the
    /// package is written restores the old key (zero-changed node identity).
    pub fn finalize_wilaya(
        &mut self,
        signed_cert: &IdentityCertificate,
        rotation_package_path: &Path,
        settings: &Settings,
        crypto_port: &AgeFileEncryptionProvider,
    ) -> AppResult<RotationFinalizeOutcome> {
        signed_cert.require_signed()?;
        if signed_cert.subject_type != SubjectType::Wilaya {
            return Err(Self::permitted(
                "finalize_wilaya_rotation requires a WILAYA certificate",
            ));
        }

        let stored = self
            .db
            .executor()
            .identity_store()
            .get_active_by_subject_type(SubjectType::Wilaya)?
            .ok_or_else(|| Self::permitted("No ACTIVE WILAYA identity to rotate"))?;

        let pending = self.node_key_store.read_pending()?;
        let Some(pending) = pending else {
            // Idempotent re-presentation of an already-finalized rotation.
            if signed_cert.is_identical_to(&stored) {
                return Ok(RotationFinalizeOutcome::AlreadyCompleted {
                    certificate: stored,
                });
            }
            return Err(Self::permitted(
                "No staged rotation key — run begin_wilaya_rotation first",
            ));
        };

        let root_public_key = resolve_root_public_key()?;
        match IdentityRotationService::verify_finalize(
            signed_cert,
            &stored,
            &pending,
            &root_public_key,
        )? {
            FinalizeVerdict::Replay { certificate } => {
                return Ok(RotationFinalizeOutcome::AlreadyCompleted { certificate });
            }
            FinalizeVerdict::Accept { .. } => {}
        }

        // Keep the pre-promotion secret so a failure after promotion can be
        // rolled back (the node must never carry an R5-broken key/cert pair).
        let old_key = self.node_key_store.read()?;

        // 1. Rotation Trust Package, signed by the OLD (still ACTIVE) key.
        let source_node_id = resolve_export_source_node_id(self.db.executor(), settings)?;
        let payload = TrustPackagePayload {
            certificates: vec![signed_cert.clone()],
            revocations: vec![],
        };
        IdentitySignedExportService::new(&*self.db, self.node_key_store)
            .export_v2_package(
                payload,
                &source_node_id,
                TRUST_PACKAGE_KIND,
                rotation_package_path,
                SubjectType::Wilaya,
                crypto_port,
            )
            .map_err(|e| {
                log::warn!(
                    target: "grpc::identity",
                    "WILAYA rotation trust package write failed (zero-changed): {e}"
                );
                e
            })?;

        // 2. Promote the staged secret.
        self.node_key_store.promote_pending()?;

        // 3. Install the new ACTIVE certificate; restore the old key on failure.
        let now = Utc::now().to_rfc3339();
        if let Err(e) = self.install_active(signed_cert, &now) {
            if let Err(restore_err) = self.node_key_store.write(&old_key) {
                log::error!(
                    target: "grpc::identity",
                    "WILAYA rotation install failed AND old-key restore failed: {e} / {restore_err}"
                );
            }
            return Err(e);
        }

        let operation = Self::derive_operation(&stored, signed_cert);
        log::info!(
            target: "grpc::identity",
            "WILAYA rotation completed: op={operation} identity={} path={}",
            signed_cert.identity_id,
            rotation_package_path.display()
        );
        Ok(RotationFinalizeOutcome::Completed {
            certificate: signed_cert.clone(),
            operation,
        })
    }

    /// Sign a UNIT rotation CSR on the WILAYA node (RFC §3.12 D2).
    ///
    /// The signing path is shared with the bootstrap CSR path
    /// (`IdentityProvisioningService::sign_unit_identity_request`): the ACTIVE
    /// local WILAYA signs, never any other identity. After signing, the
    /// certificate is recorded in the WILAYA-side Identity Store as Issuer
    /// Local State — a best-effort projection, NOT a distribution channel. A
    /// credential rollback/zero-generation conflict skips the registration
    /// without blocking the signing.
    pub fn sign_unit_rotation(
        &mut self,
        request: &IdentityCertificate,
    ) -> AppResult<SignedUnitRotation> {
        let signed = IdentityProvisioningService::new(&mut *self.db)
            .sign_unit_identity_request(request, self.node_key_store)?;

        let operation = {
            let prior = self
                .db
                .executor()
                .identity_store()
                .get_active_by_subject(SubjectType::Unit, &signed.subject_id)?;
            match prior {
                Some(p) => Self::derive_operation(&p, &signed),
                None => RotationOperation::ReIssue,
            }
        };

        let now = Utc::now().to_rfc3339();
        if let Err(e) = self.register_best_effort(&signed, &now) {
            log::warn!(
                target: "grpc::identity",
                "best-effort WILAYA-side registration skipped for UNIT {}: {e}",
                signed.subject_id
            );
        }

        Ok(SignedUnitRotation {
            certificate: signed,
            operation,
        })
    }

    /// Finalize the UNIT rotation with the WILAYA-signed certificate.
    ///
    /// The issuer is resolved via `issuer_identity_id` and verified (exists +
    /// ACTIVE + WILAYA), mirroring `finalize_unit_provision`. On Accept:
    /// promote the staged secret, then install the new ACTIVE UNIT certificate.
    pub fn finalize_unit(
        &mut self,
        signed_cert: &IdentityCertificate,
    ) -> AppResult<RotationFinalizeOutcome> {
        signed_cert.require_signed()?;
        if signed_cert.subject_type != SubjectType::Unit {
            return Err(Self::permitted(
                "finalize_unit_rotation requires a UNIT certificate",
            ));
        }

        let stored = self
            .db
            .executor()
            .identity_store()
            .get_active_by_subject(SubjectType::Unit, &signed_cert.subject_id)?
            .ok_or_else(|| Self::permitted("No ACTIVE UNIT identity to rotate"))?;

        let pending = self.node_key_store.read_pending()?;
        let Some(pending) = pending else {
            if signed_cert.is_identical_to(&stored) {
                return Ok(RotationFinalizeOutcome::AlreadyCompleted {
                    certificate: stored,
                });
            }
            return Err(Self::permitted(
                "No staged rotation key — run begin_unit_rotation first",
            ));
        };

        let issuer_identity_id = signed_cert.issuer_identity_id.ok_or_else(|| {
            Self::permitted("A UNIT rotation certificate MUST be issued by a WILAYA identity")
        })?;
        let issuer = self
            .db
            .executor()
            .identity_store()
            .get_by_identity_id(&issuer_identity_id)?
            .ok_or_else(|| {
                Self::permitted(format!(
                    "The issuer identity {issuer_identity_id} does not exist in the Identity Store"
                ))
            })?;
        if issuer.status != CredentialStatus::Active {
            return Err(Self::permitted(
                "The UNIT rotation certificate issuer MUST be an ACTIVE WILAYA identity",
            ));
        }
        if issuer.subject_type != SubjectType::Wilaya {
            return Err(Self::permitted(
                "The UNIT rotation certificate issuer MUST be a WILAYA identity",
            ));
        }

        match IdentityRotationService::verify_finalize(
            signed_cert,
            &stored,
            &pending,
            &issuer.public_key,
        )? {
            FinalizeVerdict::Replay { certificate } => {
                return Ok(RotationFinalizeOutcome::AlreadyCompleted { certificate });
            }
            FinalizeVerdict::Accept { .. } => {}
        }

        let old_key = self.node_key_store.read()?;
        self.node_key_store.promote_pending()?;

        let now = Utc::now().to_rfc3339();
        if let Err(e) = self.install_active(signed_cert, &now) {
            if let Err(restore_err) = self.node_key_store.write(&old_key) {
                log::error!(
                    target: "grpc::identity",
                    "UNIT rotation install failed AND old-key restore failed: {e} / {restore_err}"
                );
            }
            return Err(e);
        }

        let operation = Self::derive_operation(&stored, signed_cert);
        log::info!(
            target: "grpc::identity",
            "UNIT rotation completed: op={operation} identity={}",
            signed_cert.identity_id
        );
        Ok(RotationFinalizeOutcome::Completed {
            certificate: signed_cert.clone(),
            operation,
        })
    }

    /// Install a certificate respecting Invariant 6 (one ACTIVE per subject):
    /// a prior ACTIVE row with a DIFFERENT `identity_id` is superseded in the
    /// same transaction before the new ACTIVE row is written. Same-identity
    /// rotations update the row in place (atomic upsert).
    fn install_active(&mut self, certificate: &IdentityCertificate, now: &str) -> AppResult<()> {
        self.db.with_transaction(|tx| {
            let store = tx.identity_store();
            if let Some(prior) =
                store.get_active_by_subject(certificate.subject_type, &certificate.subject_id)?
            {
                if prior.identity_id != certificate.identity_id {
                    let mut superseded = prior.clone();
                    superseded.status = CredentialStatus::Superseded;
                    store.upsert(&superseded, now)?;
                }
            }
            store.upsert(certificate, now)?;
            Ok(())
        })
    }

    /// Best-effort WILAYA-side credential registration (Issuer Local State).
    ///
    /// Mirrors the Trust Package per-certificate gate (RFC §3.4.2): Accept →
    /// install, Replay → idempotent no-op, Rollback/RejectZero → error (the
    /// caller logs and skips — the signing itself already succeeded).
    fn register_best_effort(
        &mut self,
        certificate: &IdentityCertificate,
        now: &str,
    ) -> AppResult<()> {
        let stored_generation = self
            .db
            .executor()
            .identity_store()
            .max_generation_for_credential(&certificate.credential_id)?;
        match CredentialGuard::check(
            &certificate.credential_id.to_string(),
            certificate.generation,
            stored_generation,
        ) {
            CredentialVerdict::Accept { .. } => self.install_active(certificate, now),
            CredentialVerdict::Replay { .. } => Ok(()),
            CredentialVerdict::Rollback { .. } | CredentialVerdict::RejectZero { .. } => {
                Err(Self::permitted(
                    "WILAYA-side registration would roll back the stored UNIT credential",
                ))
            }
        }
    }

    fn derive_operation(
        previous: &IdentityCertificate,
        next: &IdentityCertificate,
    ) -> RotationOperation {
        if previous.credential_id == next.credential_id {
            RotationOperation::Rotate
        } else {
            RotationOperation::ReIssue
        }
    }

    fn generate_keypair() -> ([u8; 32], Ed25519SigningProvider) {
        use rand::RngCore;
        let mut secret = [0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut secret);
        let signer = Ed25519SigningProvider::new(secret);
        (secret, signer)
    }

    fn permitted(message: impl Into<String>) -> AppError {
        AppError::BusinessLogic(BusinessLogicError::OperationNotPermitted {
            message: message.into(),
        })
    }
}

impl crate::architecture::Service for IdentityRotationCoordinator<'_> {}
