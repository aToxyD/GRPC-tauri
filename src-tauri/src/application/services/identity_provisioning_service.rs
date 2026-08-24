//! Identity provisioning application service (B3).
//!
//! RFC 2026-08-04-node-identity-trust §3.2, §3.8 / ADR-0038 / ADR-0039 §3.
//!
//! - WILAYA: keypair generated on-node (private key never leaves), certificate
//!   signed by the authority Root (offline in production; injected in tests).
//!   The secret key is stored x25519-encrypted via `NodeKeyStore`.
//! - ADMIN: certificate issued and signed by the local WILAYA node (no
//!   self-signing, ADR-0039 §5); the private key is written to the portable
//!   `.adminkey` (age::scrypt, operator passphrase) via `AdminKeyProvider`.

use crate::application::services::{
    identity_authentication_policy::{AdminCredentialState, IdentityAuthenticationPolicy},
    NodeIdentityResolver,
};
use crate::application::sync_integrity::credential_guard::{CredentialGuard, CredentialVerdict};
use crate::db::Database;
use crate::domain::identity::{
    AdminKeyFile, CredentialStatus, Ed25519CertificateSignature, IdentityCertificate,
    IdentitySignatureVerifier, IdentitySigner, IdentityStorePort, SubjectType,
    ADMINKEY_FORMAT_VERSION, IDENTITY_ALGORITHM_PROFILE_ED25519, SIGNATURE_VERSION_ED25519,
};
use crate::errors::{AppError, AppResult, BusinessLogicError, ValidationError};
use crate::infrastructure::identity::{resolve_root_public_key, AdminKeyProvider, NodeKeyStore};
use crate::infrastructure::security::Ed25519SigningProvider;
use crate::models::{UnitNodePackage, UserRole};
use crate::repositories::executor::DbExecutor;
use crate::repositories::RepositoryProvider;
use uuid::Uuid;

/// Bootstrap admin username — the single identity-linked operator.
pub const BOOTSTRAP_ADMIN_USERNAME: &str = "admin";

/// Outcome of `finalize_wilaya_provision`.
///
/// Mirrors `PackageImportResult` style: identical re-presentation of the same
/// certificate yields `AlreadyProvisioned` with ZERO writes (idempotent import);
/// any real difference fails closed instead of mutating state.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum FinalizeWilayaProvisionResult {
    Provisioned(IdentityCertificate),
    AlreadyProvisioned(IdentityCertificate),
}

/// Outcome of `finalize_unit_provision`.
///
/// Same B5 idempotency semantics as `FinalizeWilayaProvisionResult`: an
/// identical re-presentation of the ACTIVE UNIT certificate yields
/// `AlreadyProvisioned` with ZERO writes; any real difference fails closed.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum FinalizeUnitProvisionResult {
    Provisioned(IdentityCertificate),
    AlreadyProvisioned(IdentityCertificate),
}

/// Packaged UNIT identity carried inside an encrypted `.unit` (ADR-0044
/// packaged-identity bootstrap).
///
/// The WILAYA generates the UNIT Ed25519 keypair in memory at export time,
/// signs the UNIT certificate with the ACTIVE local WILAYA identity, and embeds
/// BOTH the signed certificate and the 32-byte secret into the encrypted
/// package. On the UNIT node the secret is written to the local `NodeKeyStore`
/// and the certificate installed inside the import audit transaction.
#[derive(Debug, Clone)]
pub struct PackagedUnitIdentity {
    pub certificate: IdentityCertificate,
    pub secret_key: [u8; 32],
}

/// Provisions WILAYA node identities and issues ADMIN certificates.
pub struct IdentityProvisioningService<'a> {
    db: &'a mut Database,
}

impl<'a> IdentityProvisioningService<'a> {
    pub fn new(db: &'a mut Database) -> Self {
        Self { db }
    }

    /// Generate an Ed25519 keypair from a secure OS entropy source.
    fn generate_keypair() -> (
        [u8; 32],
        crate::infrastructure::security::Ed25519SigningProvider,
    ) {
        use rand::RngCore;
        let mut secret = [0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut secret);
        let signer = crate::infrastructure::security::Ed25519SigningProvider::new(secret);
        (secret, signer)
    }

    /// Provision (or re-provision) the local WILAYA node identity.
    ///
    /// The authority Root signs the certificate (`authority_signer`); the
    /// WILAYA private key never leaves the node and is stored encrypted at rest
    /// through `node_key_store`.
    pub fn provision_wilaya(
        &mut self,
        subject_id: Uuid,
        authority_signer: &dyn IdentitySigner,
        node_key_store: &NodeKeyStore,
        now: &str,
    ) -> AppResult<IdentityCertificate> {
        let (secret, signer) = Self::generate_keypair();
        node_key_store.write(&secret)?;

        let mut cert = IdentityCertificate {
            identity_id: Uuid::new_v4(),
            subject_type: SubjectType::Wilaya,
            subject_id,
            // Offline Authority Root (escrow): the root identity is not a node.
            issuer_identity_id: None,
            credential_id: Uuid::new_v4(),
            generation: 1,
            status: CredentialStatus::Active,
            public_key: signer.public_key(),
            algorithm_version: SIGNATURE_VERSION_ED25519,
            not_after: None,
            package_sequence: None,
            signature: None,
        };
        let signature = authority_signer
            .sign_certificate(&cert)
            .map_err(|e| AppError::Internal(format!("WILAYA cert signing failed: {e}")))?;
        cert.signature = Some(self.wrap_signature(signature)?);
        cert.require_signed()?;

        let store = self.db.executor().identity_store();
        store.upsert(&cert, now)?;
        Ok(cert)
    }

    /// Issue an ADMIN certificate signed by the local WILAYA node.
    ///
    /// Writes the certificate to the Identity Store and the encrypted private
    /// key to the portable `.adminkey` (passphrase-protected). The ADMIN
    /// private key is only ever recoverable via the operator passphrase.
    ///
    /// Issuance routes through the shared `sign_identity_request` path
    /// (RFC §3.12 D2): the declared issuer MUST be the ACTIVE local WILAYA and
    /// MUST be the signer (R5).
    pub fn issue_admin(
        &mut self,
        subject_id: Uuid,
        passphrase: &str,
        wilaya_identity_id: Uuid,
        node_key_store: &NodeKeyStore,
        adminkey_provider: &AdminKeyProvider,
        now: &str,
    ) -> AppResult<IdentityCertificate> {
        let store = self.db.executor().identity_store();
        let wilaya = store
            .get_active_by_subject_type(SubjectType::Wilaya)?
            .ok_or_else(|| {
                AppError::BusinessLogic(BusinessLogicError::OperationNotPermitted {
                    message: "An ACTIVE WILAYA identity is required before issuing an ADMIN key"
                        .into(),
                })
            })?;
        if wilaya.identity_id != wilaya_identity_id {
            return Err(AppError::BusinessLogic(
                BusinessLogicError::OperationNotPermitted {
                    message: "The declared issuer is not the ACTIVE local WILAYA identity".into(),
                },
            ));
        }

        let wilaya_secret = node_key_store.read()?;
        let wilaya_signer = Ed25519SigningProvider::new(wilaya_secret);

        let (admin_secret, admin_signer) = Self::generate_keypair();

        let request = IdentityCertificate {
            identity_id: Uuid::new_v4(),
            subject_type: SubjectType::Admin,
            subject_id,
            issuer_identity_id: None,
            credential_id: Uuid::new_v4(),
            generation: 1,
            status: CredentialStatus::Active,
            public_key: admin_signer.public_key(),
            algorithm_version: SIGNATURE_VERSION_ED25519,
            not_after: None,
            package_sequence: None,
            signature: None,
        };
        let cert = self.sign_identity_request(&request, &wilaya, &wilaya_signer)?;

        store.upsert(&cert, now)?;

        let encrypted_key = adminkey_provider.encrypt_private_key(&admin_secret, passphrase)?;
        let adminkey = AdminKeyFile {
            format_version: ADMINKEY_FORMAT_VERSION,
            algorithm_version: IDENTITY_ALGORITHM_PROFILE_ED25519,
            certificate: cert.clone(),
            encrypted_private_key: encrypted_key,
        };
        adminkey_provider.write(&adminkey)?;

        Ok(cert)
    }

    /// The SINGLE node-issuance path (RFC §3.12 D2).
    ///
    /// Owns `issuer_identity_id` binding, signature generation, and credential
    /// metadata for every node-issued certificate (ADMIN, UNIT, and future
    /// Rotate/Re-Issue/Recovery). Fail-closed:
    /// - the request MUST be unsigned (`signature == None`);
    /// - `generation >= 1`;
    /// - `status == Active`;
    /// - R5 for the issuer: `signer.public_key() == issuer_identity.public_key`
    ///   (the declared issuer MUST be the signing identity).
    ///
    /// The certificate is bound to the issuer via
    /// `issuer_identity_id = issuer_identity.identity_id` and signed over its
    /// canonical bytes (ADR-0039 §5). WILAYA (Root-issued) certificates do NOT
    /// use this path — their issuer is the offline Root (`None`).
    pub fn sign_identity_request(
        &self,
        request: &IdentityCertificate,
        issuer_identity: &IdentityCertificate,
        signer: &dyn IdentitySigner,
    ) -> AppResult<IdentityCertificate> {
        if request.signature.is_some() {
            return Err(AppError::BusinessLogic(
                BusinessLogicError::OperationNotPermitted {
                    message:
                        "The request is already signed; a CSR must be unsigned at signing time"
                            .into(),
                },
            ));
        }
        if request.generation < 1 {
            return Err(AppError::BusinessLogic(
                BusinessLogicError::OperationNotPermitted {
                    message: "Identity generation must be >= 1".into(),
                },
            ));
        }
        if request.status != CredentialStatus::Active {
            return Err(AppError::BusinessLogic(
                BusinessLogicError::OperationNotPermitted {
                    message: "Only ACTIVE requests can be issued".into(),
                },
            ));
        }
        if signer.public_key() != issuer_identity.public_key {
            return Err(AppError::BusinessLogic(
                BusinessLogicError::OperationNotPermitted {
                    message: "R5: the declared issuer is not the signing identity".into(),
                },
            ));
        }

        let mut signed = request.clone();
        signed.issuer_identity_id = Some(issuer_identity.identity_id);
        let signature = signer
            .sign_certificate(&signed)
            .map_err(|e| AppError::Internal(format!("Certificate signing failed: {e}")))?;
        signed.signature = Some(self.wrap_signature(signature)?);
        signed.require_signed()?;
        Ok(signed)
    }

    fn wrap_signature(&self, bytes: Vec<u8>) -> AppResult<Ed25519CertificateSignature> {
        Ed25519CertificateSignature::try_from(bytes)
            .map_err(|e| AppError::Internal(format!("Invalid certificate signature: {e}")))
    }

    /// Generate an offline identity request (CSR) for `subject_type`.
    ///
    /// RFC 2026-08-04-node-identity-trust §3.6 / §3.12 — "Root signs fully
    /// offline": on-node keypair generation, the secret is persisted
    /// x25519-encrypted via `node_key_store` (it NEVER leaves the node), and an
    /// UNSIGNED certificate (the CSR) is returned for the operator to carry to
    /// the issuer. Nothing else is persisted. Only valid from `Uninitialized`
    /// (one-way transition guard): re-entry is rejected once a node key exists.
    ///
    /// The CSR is a transport artifact (serialized JSON), never stored. The
    /// issuer is bound at signing time: `None` for WILAYA (offline Root), the
    /// WILAYA identity for UNIT (see `sign_unit_identity_request`).
    pub fn generate_identity_request(
        &self,
        subject_type: SubjectType,
        subject_id: Uuid,
        node_key_store: &NodeKeyStore,
    ) -> AppResult<IdentityCertificate> {
        if node_key_store.exists() {
            return Err(AppError::BusinessLogic(BusinessLogicError::OperationNotPermitted {
                message: "A node signing key already exists; bootstrap already began (state is no longer Uninitialized)".into(),
            }));
        }
        let (secret, signer) = Self::generate_keypair();
        node_key_store.write(&secret)?;
        Ok(IdentityCertificate {
            identity_id: Uuid::new_v4(),
            subject_type,
            subject_id,
            issuer_identity_id: None,
            credential_id: Uuid::new_v4(),
            generation: 1,
            status: CredentialStatus::Active,
            public_key: signer.public_key(),
            algorithm_version: SIGNATURE_VERSION_ED25519,
            not_after: None,
            package_sequence: None,
            signature: None,
        })
    }

    /// WILAYA bootstrap request — thin wrapper over `generate_identity_request`.
    pub fn generate_wilaya_request(
        &self,
        subject_id: Uuid,
        node_key_store: &NodeKeyStore,
    ) -> AppResult<IdentityCertificate> {
        self.generate_identity_request(SubjectType::Wilaya, subject_id, node_key_store)
    }

    /// Generate the UNIT identity request for a **packaged** bootstrap (ADR-0044
    /// packaged-identity flow) WITHOUT persisting anything on the WILAYA node.
    ///
    /// Unlike `generate_identity_request` (the UNIT-side CSR flow), this runs on
    /// the WILAYA node and returns the freshly generated secret alongside the
    /// UNSIGNED UNIT certificate. The secret is held in memory only — it is
    /// embedded into the encrypted `.unit` by the exporter and is NEVER written
    /// to the WILAYA `NodeKeyStore`. The subject_id is the local unit row on the
    /// WILAYA (the issuer validates it at signing time).
    pub fn generate_unit_identity_request_for_package(
        &self,
        subject_id: Uuid,
    ) -> AppResult<([u8; 32], IdentityCertificate)> {
        let (secret, signer) = Self::generate_keypair();
        let request = IdentityCertificate {
            identity_id: Uuid::new_v4(),
            subject_type: SubjectType::Unit,
            subject_id,
            issuer_identity_id: None,
            credential_id: Uuid::new_v4(),
            generation: 1,
            status: CredentialStatus::Active,
            public_key: signer.public_key(),
            algorithm_version: SIGNATURE_VERSION_ED25519,
            not_after: None,
            package_sequence: None,
            signature: None,
        };
        Ok((secret, request))
    }

    /// Resolve the local UNIT `subject_id` from the persisted unit identity.
    ///
    /// RFC §3.12 D2 — the CSR must represent the node that created it: the
    /// subject_id is resolved LOCALLY (`settings` unit code → local `units`
    /// row), never imposed by the issuer. Fail-closed: a missing unit row means
    /// the base node package has not been imported.
    pub fn resolve_local_unit_subject_id(&self) -> AppResult<Uuid> {
        let unit_code = self
            .db
            .executor()
            .settings()
            .get_first_unit_code()?
            .ok_or_else(|| {
                AppError::BusinessLogic(BusinessLogicError::OperationNotPermitted {
                    message: "No unit configured on this node; import the base node package first"
                        .into(),
                })
            })?;
        let unit = self
            .db
            .executor()
            .units()
            .get_unit_by_code(&unit_code)?
            .ok_or_else(|| {
                AppError::BusinessLogic(BusinessLogicError::OperationNotPermitted {
                    message: format!("No local units row matches unit code '{unit_code}'"),
                })
            })?;
        Uuid::parse_str(&unit.id).map_err(|e| {
            AppError::Internal(format!("units.id '{}' is not a valid UUID: {e}", unit.id))
        })
    }

    /// Sign a UNIT CSR on the local WILAYA node (RFC §3.12 D2).
    ///
    /// Fail-closed:
    /// - the request MUST be a UNIT CSR;
    /// - the CSR `subject_id` MUST correspond to a known local unit (the issuer
    ///   validates the subject; it never overrides it);
    /// - the local WILAYA signer is resolved via `NodeIdentityResolver`
    ///   (ACTIVE cert + R5);
    /// - issuance flows through `sign_identity_request`.
    pub fn sign_unit_identity_request(
        &self,
        request: &IdentityCertificate,
        node_key_store: &NodeKeyStore,
    ) -> AppResult<IdentityCertificate> {
        if request.subject_type != SubjectType::Unit {
            return Err(AppError::BusinessLogic(
                BusinessLogicError::OperationNotPermitted {
                    message: "sign_unit_identity_request requires a UNIT request".into(),
                },
            ));
        }
        let unit_id = request.subject_id.to_string();
        let unit = self
            .db
            .executor()
            .units()
            .get_unit(&unit_id)?
            .ok_or_else(|| {
                AppError::BusinessLogic(BusinessLogicError::OperationNotPermitted {
                    message: format!(
                        "CSR subject_id {unit_id} does not correspond to a known local unit"
                    ),
                })
            })?;
        let _ = unit;

        let resolved = NodeIdentityResolver::resolve_local_signer(
            self.db,
            node_key_store,
            SubjectType::Wilaya,
        )?
        .ok_or_else(|| {
            AppError::BusinessLogic(BusinessLogicError::OperationNotPermitted {
                message: "No ACTIVE WILAYA identity on this node; cannot sign UNIT requests".into(),
            })
        })?;
        self.sign_identity_request(request, &resolved.certificate, &resolved.signer)
    }

    /// Sign a UNIT **bootstrap** CSR on the local WILAYA node (RFC §3.12 D2),
    /// hardened for SEC-004-02/03/04.
    ///
    /// This is the BOOTSTRAP-ONLY entry point. It MUST NOT be used by the
    /// rotation flow: `IdentityRotationCoordinator::sign_unit_rotation` reuses
    /// the shared `sign_unit_identity_request` signing path, where an existing
    /// ACTIVE credential is legitimately superseded during RE-ISSUE. Fail-closed:
    /// - all shared CSR checks of `sign_unit_identity_request` (subject, issuer,
    ///   R5, unsigned, generation >= 1, ACTIVE);
    /// - duplicate ACTIVE guard: if an ACTIVE UNIT identity already exists for
    ///   `subject_id`, bootstrap issuance is DENIED with zero mutation;
    /// - REVOKED/SUPERSEDED/EXPIRED credential histories do NOT block bootstrap
    ///   re-issuance — the guard is ACTIVE-status only, matching the repository
    ///   `get_active_by_subject` semantics (SEC-004-03);
    /// - after signing, the issued certificate is registered in the WILAYA
    ///   Identity Store (Issuer Local State) through the CredentialGuard gate:
    ///   Accept → install, Replay → idempotent no-op, Rollback/RejectZero →
    ///   fail-closed error (no signed UNIT certificate escapes without a
    ///   WILAYA-side record).
    pub fn sign_unit_bootstrap_request(
        &self,
        request: &IdentityCertificate,
        node_key_store: &NodeKeyStore,
        now: &str,
    ) -> AppResult<IdentityCertificate> {
        let store = self.db.executor().identity_store();
        if let Some(existing) =
            store.get_active_by_subject(SubjectType::Unit, &request.subject_id)?
        {
            return Err(AppError::BusinessLogic(
                BusinessLogicError::OperationNotPermitted {
                    message: format!(
                        "An ACTIVE UNIT identity already exists for this unit (credential {}); duplicate bootstrap issuance is denied — use the rotation flow",
                        existing.credential_id
                    ),
                },
            ));
        }

        let signed = self.sign_unit_identity_request(request, node_key_store)?;

        let stored_generation = store.max_generation_for_credential(&signed.credential_id)?;
        match CredentialGuard::check(
            &signed.credential_id.to_string(),
            signed.generation,
            stored_generation,
        ) {
            CredentialVerdict::Accept { .. } => {
                store.upsert(&signed, now)?;
            }
            CredentialVerdict::Replay { .. } => {
                // Idempotent: the identical credential is already recorded as
                // WILAYA-side Issuer Local State.
            }
            CredentialVerdict::Rollback { .. } | CredentialVerdict::RejectZero { .. } => {
                return Err(AppError::BusinessLogic(
                    BusinessLogicError::OperationNotPermitted {
                        message: format!(
                            "WILAYA-side registration would roll back or invalidate the stored UNIT credential {}; bootstrap issuance aborted",
                            signed.credential_id
                        ),
                    },
                ));
            }
        }

        Ok(signed)
    }

    /// Finalize the offline WILAYA bootstrap with the Root-signed certificate (B5).
    ///
    /// Verification (all MUST hold, fail-closed):
    /// - the certificate is signed by the Authority Root (`resolve_root_public_key`);
    /// - the certificate public key matches the node signing key (no cross-device swap);
    /// - the issuer is the offline Root (`None`), never a node.
    ///
    /// Idempotency: an identical re-presentation of the currently ACTIVE WILAYA
    /// certificate returns `AlreadyProvisioned` with ZERO writes (no audit, no
    /// timestamps, no row mutation). A *different* WILAYA certificate for an
    /// already-provisioned node is rejected — recovery flows are out of scope.
    pub fn finalize_wilaya_provision(
        &mut self,
        signed_cert: &IdentityCertificate,
        node_key_store: &NodeKeyStore,
        now: &str,
    ) -> AppResult<FinalizeWilayaProvisionResult> {
        signed_cert.require_signed()?;
        if signed_cert.subject_type != SubjectType::Wilaya {
            return Err(AppError::BusinessLogic(
                BusinessLogicError::OperationNotPermitted {
                    message: "finalize_wilaya_provision requires a WILAYA certificate".into(),
                },
            ));
        }
        if signed_cert.status != CredentialStatus::Active {
            return Err(AppError::BusinessLogic(
                BusinessLogicError::OperationNotPermitted {
                    message: "finalize_wilaya_provision requires an ACTIVE certificate".into(),
                },
            ));
        }
        if signed_cert.issuer_identity_id.is_some() {
            return Err(AppError::BusinessLogic(BusinessLogicError::OperationNotPermitted {
                message: "WILAYA certificates MUST be issued by the offline Authority Root (issuer None)".into(),
            }));
        }

        let root_public_key = resolve_root_public_key()?;
        let signature = signed_cert.signature.ok_or_else(|| {
            AppError::Internal("Root-signed certificate missing signature".into())
        })?;
        let verifier = crate::infrastructure::security::Ed25519SignatureVerifier;
        let root_signature_valid = verifier
            .verify_certificate(signed_cert, &root_public_key, &signature)
            .map_err(|e| AppError::Internal(format!("Root signature verification failed: {e}")))?;
        if !root_signature_valid {
            return Err(AppError::BusinessLogic(
                BusinessLogicError::OperationNotPermitted {
                    message: "Certificate signature is not valid for the Authority Root".into(),
                },
            ));
        }

        let node_secret = node_key_store.read()?;
        let node_signer = crate::infrastructure::security::Ed25519SigningProvider::new(node_secret);
        if signed_cert.public_key != node_signer.public_key() {
            return Err(AppError::BusinessLogic(
                BusinessLogicError::OperationNotPermitted {
                    message: "Certificate public key does not match the node signing key".into(),
                },
            ));
        }

        let store = self.db.executor().identity_store();
        if let Some(existing) = store.get_active_by_subject_type(SubjectType::Wilaya)? {
            if existing.is_identical_to(signed_cert) {
                return Ok(FinalizeWilayaProvisionResult::AlreadyProvisioned(existing));
            }
            return Err(AppError::BusinessLogic(BusinessLogicError::OperationNotPermitted {
                message: "An ACTIVE WILAYA identity already exists and differs from the presented certificate".into(),
            }));
        }

        store.upsert(signed_cert, now)?;
        Ok(FinalizeWilayaProvisionResult::Provisioned(
            signed_cert.clone(),
        ))
    }

    /// Issue the FIRST ADMIN key linked to the bootstrap operator (B5).
    ///
    /// SEC-002 + SEC-002-R hardening — canonical identity, one-time gate,
    /// crash-consistent ordering, and a recovery carve-out:
    /// - **Canonical identity (SEC-002-B):** the ceremony accepts ONLY
    ///   `BOOTSTRAP_ADMIN_USERNAME` (the seeded fleet-wide `admin`). The
    ///   frontend is NOT trusted; any other username is rejected here, so a
    ///   mis-typed or injected name can never mint an out-of-band ACTIVE ADMIN.
    /// - **One-time gate (SEC-002-A):** an ACTIVE WILAYA MUST exist (the local
    ///   issuer), and the ceremony rejects as soon as an ACTIVE ADMIN identity
    ///   is usable — the on-disk `.adminkey` embeds exactly that certificate
    ///   (unified `AdminCredentialState`, shared with the password login gate).
    /// - **Recovery carve-out (SEC-002-09 + SEC-002-R):** when an ACTIVE ADMIN
    ///   certificate exists but the node cannot authenticate with it
    ///   (`.adminkey` missing or its embedded certificate differs), the
    ///   canonical ceremony re-provisions and SUPERSEDES the unusable
    ///   certificate inside the same transaction, so the DB invariant "at most
    ///   one ACTIVE ADMIN" (migration 008) always holds and the crash state
    ///   never becomes a permanent lockout. **SEC-002-R:** because this branch
    ///   re-issues operator credentials, it is gated at the COMMAND boundary —
    ///   it requires an authenticated ADMIN session (`Action::AdminOnly`) plus
    ///   recovery rate limiting. The first-admin branch (no ACTIVE ADMIN) stays
    ///   pre-auth. A straight service call (integration tests, internal tooling)
    ///   bypasses that command gate by design; the enforced entry point is the
    ///   `issue_first_admin_key` command.
    /// - **Crash-consistent ordering (SEC-002-C):** the `.adminkey` is written
    ///   (atomically, tmp+rename) BEFORE the DB transaction commits. The DB is
    ///   the last committed step, so `certificate present ⇒ .adminkey present`.
    ///   A crash between the two leaves only an inert orphan `.adminkey` (no
    ///   certificate → no ACTIVE ADMIN → state stays `WilayaActive`) that a
    ///   retry atomically overwrites.
    /// - **Documented limitation (SEC-002-R):** the ceremony provisions an
    ///   identity-only admin (EMPTY password hash). If the operator never set a
    ///   fleet `admin` password (B8) and the `.adminkey` is permanently lost,
    ///   password-based recovery is UNAVAILABLE by design — the sanctioned
    ///   offline/Root recovery path (re-provision via the operator-facing flow)
    ///   is the documented recovery route. SEC-002-R never makes the password
    ///   mandatory; it only requires an authenticated ADMIN session when an
    ///   ACTIVE ADMIN exists.
    ///
    /// Links the `users` row for `subject_username`:
    /// - existing row → credential material is untouched (additive legacy window:
    ///   a pre-existing password hash stays valid; Challenge–Response is added);
    /// - missing row → created identity-only (EMPTY hash) inside the same
    ///   transaction that persists the certificate.
    ///
    /// The private key goes to the portable `.adminkey` (age::scrypt) ONLY.
    pub fn issue_first_admin_key(
        &mut self,
        subject_username: &str,
        passphrase: &str,
        node_key_store: &NodeKeyStore,
        adminkey_provider: &AdminKeyProvider,
        now: &str,
    ) -> AppResult<IdentityCertificate> {
        let username = subject_username.trim();
        if username.is_empty() {
            return Err(AppError::Validation(
                crate::errors::ValidationError::Required {
                    field: "subject_username".into(),
                },
            ));
        }
        // SEC-002-B: the first ADMIN is ALWAYS the canonical `admin`. This check
        // precedes the WILAYA/global gates so a non-canonical name can never
        // trigger the recovery carve-out or mint an out-of-band ACTIVE ADMIN.
        if username != BOOTSTRAP_ADMIN_USERNAME {
            return Err(AppError::BusinessLogic(
                BusinessLogicError::OperationNotPermitted {
                    message: format!(
                        "The first ADMIN identity MUST be the canonical '{BOOTSTRAP_ADMIN_USERNAME}' username; got '{username}'"
                    ),
                },
            ));
        }
        // SEC-002-A / SEC-002-09 / SEC-002-R: global one-time gate. The single
        // source of truth for "is the ADMIN identity usable" is the unified
        // `AdminCredentialState` predicate (shared with the password login
        // policy, so the recovery ceremony and the login gate can never
        // disagree):
        // - NoActiveAdmin → first-admin bootstrap (pre-auth, unchanged).
        // - Usable       → re-issuance rejected (the node CAN authenticate with
        //                  the existing ACTIVE ADMIN identity).
        // - Missing / Mismatched → the ACTIVE ADMIN is unusable and the
        //                  operator is locked out of Challenge–Response; the
        //                  ceremony recovers by superseding the unusable
        //                  certificate inside the same transaction.
        //
        // NOTE (SEC-002-R): the session/authorization requirement for the
        // RECOVERY branch is enforced at the COMMAND boundary (the command owns
        // `AppState`; application services must not depend on the app layer).
        // A call routed straight into this service bypasses that gate by
        // design — the thin command wrapper is the enforced entry point.
        let store = self.db.executor().identity_store();
        let wilaya = store
            .get_active_by_subject_type(SubjectType::Wilaya)?
            .ok_or_else(|| {
                AppError::BusinessLogic(BusinessLogicError::OperationNotPermitted {
                    message:
                        "An ACTIVE WILAYA identity is required before issuing the first ADMIN key"
                            .into(),
                })
            })?;

        let existing_admin =
            match IdentityAuthenticationPolicy::admin_credential_state(self.db, adminkey_provider)?
            {
                AdminCredentialState::NoActiveAdmin => None,
                AdminCredentialState::Usable => {
                    let admin = store
                        .get_active_by_subject_type(SubjectType::Admin)?
                        .expect("Usable implies an ACTIVE ADMIN certificate");
                    return Err(AppError::BusinessLogic(
                        BusinessLogicError::OperationNotPermitted {
                            message: format!(
                            "An ACTIVE ADMIN identity already exists for this node (credential {})",
                            admin.credential_id
                        ),
                        },
                    ));
                }
                AdminCredentialState::MissingAdminkey
                | AdminCredentialState::MismatchedAdminkey => {
                    store.get_active_by_subject_type(SubjectType::Admin)?
                }
            };
        let recovering = existing_admin.is_some();

        // Resolve the canonical admin's `users` row. Existing row → linked
        // additively (legacy password hash preserved); missing row → created
        // identity-only INSIDE the same transaction that persists the cert.
        let (subject_id, is_new_user) = {
            use crate::infrastructure::security::node_identity_provider::NodeIdentityProvider as _;
            let node_scope = crate::infrastructure::security::SettingsNodeIdentityProvider::new(
                self.db.executor(),
            )
            .current_node_id()
            .unwrap_or_else(|_| "WILAYA".to_string());
            let users = self.db.executor().users();
            match users.get_user_by_username(username, &node_scope)? {
                Some(existing) => {
                    let parsed = Uuid::parse_str(&existing.id).map_err(|e| {
                        AppError::Internal(format!(
                            "Linked user '{}' has an invalid id '{}': {e}",
                            username, existing.id
                        ))
                    })?;
                    (parsed, false)
                }
                None => (Uuid::new_v4(), true),
            }
        };

        let wilaya_secret = node_key_store.read()?;
        let wilaya_signer =
            crate::infrastructure::security::Ed25519SigningProvider::new(wilaya_secret);

        let (admin_secret, admin_signer) = Self::generate_keypair();

        let request = IdentityCertificate {
            identity_id: Uuid::new_v4(),
            subject_type: SubjectType::Admin,
            subject_id,
            issuer_identity_id: None,
            credential_id: Uuid::new_v4(),
            generation: 1,
            status: CredentialStatus::Active,
            public_key: admin_signer.public_key(),
            algorithm_version: SIGNATURE_VERSION_ED25519,
            not_after: None,
            package_sequence: None,
            signature: None,
        };
        let cert = self.sign_identity_request(&request, &wilaya, &wilaya_signer)?;

        // SEC-002-C: persist the operator key material FIRST (atomic tmp+rename).
        let encrypted_key = adminkey_provider.encrypt_private_key(&admin_secret, passphrase)?;
        let adminkey = AdminKeyFile {
            format_version: ADMINKEY_FORMAT_VERSION,
            algorithm_version: IDENTITY_ALGORITHM_PROFILE_ED25519,
            certificate: cert.clone(),
            encrypted_private_key: encrypted_key,
        };
        adminkey_provider.write(&adminkey)?;

        // THEN commit the database side in ONE transaction (the last committed
        // step): new user row → supersede unusable admin (recovery) → cert.
        // A failure here leaves only the inert orphan `.adminkey`, which the
        // next ceremony attempt atomically overwrites (no lockout).
        self.db.with_transaction(|ex| {
            if is_new_user {
                let node_id =
                    crate::infrastructure::security::NodeIdentityProvider::current_node_id(
                        &crate::infrastructure::security::SettingsNodeIdentityProvider::new(ex),
                    )
                    .unwrap_or_else(|_| "WILAYA".to_string());
                ex.users().upsert_user(
                    &subject_id.to_string(),
                    username,
                    "",
                    UserRole::Admin,
                    &node_id,
                    now,
                )?;
            }
            if let Some(prior) = &existing_admin {
                let mut superseded = prior.clone();
                superseded.status = CredentialStatus::Superseded;
                ex.identity_store().upsert(&superseded, now)?;
            }
            ex.identity_store().upsert(&cert, now)?;
            Ok(())
        })?;

        // SEC-002-D: audit the ceremony (best-effort). Only identity ids are
        // recorded — never the passphrase, the private key, or the encrypted
        // key material.
        let audit = serde_json::json!({
            "subject_type": "ADMIN",
            "identity_id": cert.identity_id.to_string(),
            "credential_id": cert.credential_id.to_string(),
            "recovery": recovering,
        });
        if let Err(e) = crate::application::services::AuditService::new(self.db.executor())
            .log_success(
                &subject_id.to_string(),
                username,
                crate::domain::audit::AuditAction::FirstAdminProvisioned,
                crate::domain::audit::EntityType::System,
                Some(&subject_id.to_string()),
                Some(username),
                None,
                None,
                None,
                Some(audit),
            )
        {
            log::error!(
                target: "grpc::identity",
                "SEC-002: failed to audit FirstAdminProvisioned for subject {}: {e}",
                subject_id
            );
        }

        Ok(cert)
    }

    /// Finalize the UNIT bootstrap with the WILAYA-signed certificate (B6-A).
    ///
    /// The issuer is resolved strictly via
    /// `signed_cert.issuer_identity_id -> IdentityStore.get_by_identity_id`
    /// (RFC §3.12 D2 — never `get_active_by_subject_type`): the presented
    /// issuer MUST exist, be ACTIVE, and be a WILAYA subject. Verification
    /// (all MUST hold, fail-closed):
    /// - the certificate is signed by that resolved issuer (R5);
    /// - the certificate public key matches the node signing key
    ///   (no cross-device swap);
    /// - the certificate matches the node's local `units` row
    ///   (`subject_id` + `subject_type == Unit`).
    ///
    /// Idempotency (B5 style): an identical re-presentation of the currently
    /// ACTIVE UNIT certificate returns `AlreadyProvisioned` with ZERO writes;
    /// a different ACTIVE UNIT certificate is rejected (rotation flows are
    /// out of scope).
    pub fn finalize_unit_provision(
        &mut self,
        signed_cert: &IdentityCertificate,
        node_key_store: &NodeKeyStore,
        now: &str,
    ) -> AppResult<FinalizeUnitProvisionResult> {
        Self::finalize_unit_identity_core(self.db.executor(), signed_cert, node_key_store, now)
    }

    /// Shared UNIT-certificate verification + install core.
    ///
    /// Used by BOTH the legacy CSR finalize path (`finalize_unit_provision`) and
    /// the packaged-identity import path (`install_unit_identity_on_executor`)
    /// so the two cannot drift. It preserves every `finalize_unit_provision`
    /// check and adds the ADR-0039 algorithm-profile pin
    /// (`algorithm_version == 2`); every legitimate certificate in the system
    /// already carries `SIGNATURE_VERSION_ED25519`, so the pin is
    /// behavior-preserving for existing flows.
    fn finalize_unit_identity_core(
        executor: DbExecutor<'_>,
        signed_cert: &IdentityCertificate,
        node_key_store: &NodeKeyStore,
        now: &str,
    ) -> AppResult<FinalizeUnitProvisionResult> {
        signed_cert.require_signed()?;
        if signed_cert.algorithm_version != IDENTITY_ALGORITHM_PROFILE_ED25519 {
            return Err(AppError::BusinessLogic(
                BusinessLogicError::OperationNotPermitted {
                    message: format!(
                        "Unsupported identity algorithm profile {} (expected Ed25519)",
                        signed_cert.algorithm_version
                    ),
                },
            ));
        }
        if signed_cert.subject_type != SubjectType::Unit {
            return Err(AppError::BusinessLogic(
                BusinessLogicError::OperationNotPermitted {
                    message: "finalize_unit_provision requires a UNIT certificate".into(),
                },
            ));
        }
        if signed_cert.status != CredentialStatus::Active {
            return Err(AppError::BusinessLogic(
                BusinessLogicError::OperationNotPermitted {
                    message: "finalize_unit_provision requires an ACTIVE certificate".into(),
                },
            ));
        }

        let issuer_identity_id = signed_cert.issuer_identity_id.ok_or_else(|| {
            AppError::BusinessLogic(BusinessLogicError::OperationNotPermitted {
                message: "A UNIT certificate MUST be issued by a WILAYA identity".into(),
            })
        })?;
        let store = executor.identity_store();
        let issuer = store
            .get_by_identity_id(&issuer_identity_id)?
            .ok_or_else(|| {
                AppError::BusinessLogic(BusinessLogicError::OperationNotPermitted {
                    message: format!(
                        "The issuer identity {issuer_identity_id} does not exist in the Identity Store"
                    ),
                })
            })?;
        if issuer.status != CredentialStatus::Active {
            return Err(AppError::BusinessLogic(
                BusinessLogicError::OperationNotPermitted {
                    message: "The UNIT certificate issuer MUST be an ACTIVE WILAYA identity".into(),
                },
            ));
        }
        if issuer.subject_type != SubjectType::Wilaya {
            return Err(AppError::BusinessLogic(
                BusinessLogicError::OperationNotPermitted {
                    message: "The UNIT certificate issuer MUST be a WILAYA identity".into(),
                },
            ));
        }

        let signature = signed_cert.signature.ok_or_else(|| {
            AppError::Internal("WILAYA-signed certificate missing signature".into())
        })?;
        let verifier = crate::infrastructure::security::Ed25519SignatureVerifier;
        let issuer_signature_valid = verifier
            .verify_certificate(signed_cert, &issuer.public_key, &signature)
            .map_err(|e| {
                AppError::Internal(format!("Issuer signature verification failed: {e}"))
            })?;
        if !issuer_signature_valid {
            return Err(AppError::BusinessLogic(
                BusinessLogicError::OperationNotPermitted {
                    message: "Certificate signature is not valid for the declared WILAYA issuer"
                        .into(),
                },
            ));
        }

        let node_secret = node_key_store.read()?;
        let node_signer = crate::infrastructure::security::Ed25519SigningProvider::new(node_secret);
        if signed_cert.public_key != node_signer.public_key() {
            return Err(AppError::BusinessLogic(
                BusinessLogicError::OperationNotPermitted {
                    message: "Certificate public key does not match the node signing key".into(),
                },
            ));
        }

        let unit_id = signed_cert.subject_id.to_string();
        let unit = executor.units().get_unit(&unit_id)?.ok_or_else(|| {
            AppError::BusinessLogic(BusinessLogicError::OperationNotPermitted {
                message: format!(
                    "Certificate subject_id {unit_id} does not match a local unit row"
                ),
            })
        })?;
        let _ = unit;

        if let Some(existing) =
            store.get_active_by_subject(SubjectType::Unit, &signed_cert.subject_id)?
        {
            if existing.is_identical_to(signed_cert) {
                return Ok(FinalizeUnitProvisionResult::AlreadyProvisioned(existing));
            }
            return Err(AppError::BusinessLogic(BusinessLogicError::OperationNotPermitted {
                message: "An ACTIVE UNIT identity already exists and differs from the presented certificate".into(),
            }));
        }

        store.upsert(signed_cert, now)?;
        Ok(FinalizeUnitProvisionResult::Provisioned(
            signed_cert.clone(),
        ))
    }

    /// Install a packaged UNIT identity on the importing node (ADR-0044
    /// packaged-identity bootstrap).
    ///
    /// Executor-compatible twin of `finalize_unit_provision`: it runs the same
    /// shared verification core (`finalize_unit_identity_core`) so the packaged
    /// path can be invoked inside the import audit transaction with no
    /// `&mut Database` (audit re-locks the non-reentrant `Mutex`). The caller
    /// MUST have written the packaged secret to the local `NodeKeyStore` first
    /// (see `install_node_key_matching`) — the core binds the certificate to the
    /// installed key, which is exactly the "key present, identity absent"
    /// crash-residual retry path.
    pub fn install_unit_identity_on_executor(
        executor: DbExecutor<'_>,
        signed_cert: &IdentityCertificate,
        node_key_store: &NodeKeyStore,
        now: &str,
    ) -> AppResult<FinalizeUnitProvisionResult> {
        Self::finalize_unit_identity_core(executor, signed_cert, node_key_store, now)
    }

    /// Install the packaged UNIT secret into the local `NodeKeyStore` with a
    /// never-overwrite guard (ADR-0044 packaged-identity bootstrap).
    ///
    /// Fail-closed:
    /// - the secret MUST derive the certificate's public key (binding, checked
    ///   BEFORE any disk write);
    /// - key absent → written (atomic write + rename);
    /// - key present and identical → no-op;
    /// - key present but DIFFERENT → rejected (refuses to overwrite an existing
    ///   node key — recovery flows are out of scope);
    /// - corrupt/unreadable key file → fail closed (no overwrite).
    pub fn install_node_key_matching(
        node_key_store: &NodeKeyStore,
        secret_key: &[u8; 32],
        expected_public_key: &[u8],
    ) -> AppResult<()> {
        let derived = Ed25519SigningProvider::new(*secret_key).public_key();
        if derived != expected_public_key {
            return Err(AppError::BusinessLogic(
                BusinessLogicError::OperationNotPermitted {
                    message: "Packaged UNIT private key does not match its certificate public key"
                        .into(),
                },
            ));
        }
        match node_key_store.read_if_exists()? {
            None => node_key_store.write(secret_key)?,
            Some(existing) => {
                let existing_public = Ed25519SigningProvider::new(existing).public_key();
                if existing_public != derived {
                    return Err(AppError::BusinessLogic(
                        BusinessLogicError::OperationNotPermitted {
                            message:
                                "A node signing key already exists and differs from the packaged UNIT key; refusing to overwrite"
                                    .into(),
                        },
                    ));
                }
            }
        }
        Ok(())
    }

    /// Extract + pre-write validate the packaged UNIT identity from a `.unit`
    /// payload (ADR-0044 packaged-identity bootstrap, import step 8).
    ///
    /// Runs BEFORE any persistence and returns the in-memory identity material.
    /// Fail-closed:
    /// - both-or-neither: certificate and private key must be BOTH present or
    ///   BOTH absent (`None` = legacy package, unchanged import path);
    /// - the private key MUST be exactly 32 bytes;
    /// - certificate: signed, UNIT subject, expected subject_id, ACTIVE,
    ///   Ed25519 profile (`algorithm_version == 2`);
    /// - `issuer_identity_id` MUST equal the package issuer — which has already
    ///   been authenticated by `verify_v2_signature` + B8 against the installed
    ///   ACTIVE WILAYA trust anchor;
    /// - the secret MUST derive the certificate public key.
    ///
    /// The cryptographic signature verification against the resolved issuer
    /// happens inside the transaction (`finalize_unit_identity_core`), on the
    /// same snapshot as the key write and identity upsert.
    pub fn extract_packaged_unit_identity(
        package: &UnitNodePackage,
        expected_subject_id: &Uuid,
        expected_issuer_identity_id: &Uuid,
    ) -> AppResult<Option<PackagedUnitIdentity>> {
        let (cert, key) = match (&package.unit_certificate, &package.unit_private_key) {
            (Some(cert), Some(key)) => (cert, key),
            (None, None) => return Ok(None),
            _ => {
                return Err(AppError::Validation(ValidationError::InvalidFormat {
                    field: "unit_certificate/unit_private_key".into(),
                    message: "packaged identity must carry BOTH a certificate and a private key (or neither)"
                        .into(),
                }));
            }
        };

        let secret_key: [u8; 32] = key.as_slice().try_into().map_err(|_| {
            AppError::Validation(ValidationError::InvalidFormat {
                field: "unit_private_key".into(),
                message: format!(
                    "packaged UNIT private key must be exactly 32 bytes (got {})",
                    key.len()
                ),
            })
        })?;

        cert.require_signed()?;
        if cert.subject_type != SubjectType::Unit {
            return Err(AppError::BusinessLogic(
                BusinessLogicError::OperationNotPermitted {
                    message: "packaged identity certificate MUST have a UNIT subject".into(),
                },
            ));
        }
        if &cert.subject_id != expected_subject_id {
            return Err(AppError::BusinessLogic(
                BusinessLogicError::OperationNotPermitted {
                    message: format!(
                        "packaged identity subject_id {} does not match the imported unit {}",
                        cert.subject_id, expected_subject_id
                    ),
                },
            ));
        }
        if cert.status != CredentialStatus::Active {
            return Err(AppError::BusinessLogic(
                BusinessLogicError::OperationNotPermitted {
                    message: "packaged identity certificate MUST be ACTIVE".into(),
                },
            ));
        }
        if cert.algorithm_version != IDENTITY_ALGORITHM_PROFILE_ED25519 {
            return Err(AppError::BusinessLogic(
                BusinessLogicError::OperationNotPermitted {
                    message: format!(
                        "packaged identity algorithm profile {} is not Ed25519",
                        cert.algorithm_version
                    ),
                },
            ));
        }
        if cert.issuer_identity_id != Some(*expected_issuer_identity_id) {
            return Err(AppError::BusinessLogic(
                BusinessLogicError::OperationNotPermitted {
                    message: format!(
                        "packaged identity issuer {} does not match the package issuer {}",
                        cert.issuer_identity_id
                            .map(|id| id.to_string())
                            .unwrap_or_else(|| "<none>".into()),
                        expected_issuer_identity_id
                    ),
                },
            ));
        }

        let derived = Ed25519SigningProvider::new(secret_key).public_key();
        if derived != cert.public_key {
            return Err(AppError::BusinessLogic(
                BusinessLogicError::OperationNotPermitted {
                    message: "packaged UNIT private key does not match its certificate public key"
                        .into(),
                },
            ));
        }

        Ok(Some(PackagedUnitIdentity {
            certificate: cert.clone(),
            secret_key,
        }))
    }
}
