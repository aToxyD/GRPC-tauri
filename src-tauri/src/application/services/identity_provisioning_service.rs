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

use crate::db::Database;
use crate::domain::identity::{
    AdminKeyFile, CredentialStatus, Ed25519CertificateSignature, IdentityCertificate,
    IdentitySignatureVerifier, IdentitySigner, IdentityStorePort, SubjectType,
    ADMINKEY_FORMAT_VERSION, IDENTITY_ALGORITHM_PROFILE_ED25519, SIGNATURE_VERSION_ED25519,
};
use crate::errors::{AppError, AppResult, BusinessLogicError};
use crate::infrastructure::identity::{AdminKeyProvider, NodeKeyStore, resolve_root_public_key};
use crate::models::UserRole;
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

/// Provisions WILAYA node identities and issues ADMIN certificates.
pub struct IdentityProvisioningService<'a> {
    db: &'a mut Database,
}

impl<'a> IdentityProvisioningService<'a> {
    pub fn new(db: &'a mut Database) -> Self {
        Self { db }
    }

    /// Generate an Ed25519 keypair from a secure OS entropy source.
    fn generate_keypair() -> ([u8; 32], crate::infrastructure::security::Ed25519SigningProvider) {
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
    pub fn issue_admin(
        &mut self,
        subject_id: Uuid,
        passphrase: &str,
        wilaya_identity_id: Uuid,
        node_key_store: &NodeKeyStore,
        adminkey_provider: &AdminKeyProvider,
        now: &str,
    ) -> AppResult<IdentityCertificate> {
        let wilaya_secret = node_key_store.read()?;
        let wilaya_signer =
            crate::infrastructure::security::Ed25519SigningProvider::new(wilaya_secret);

        let (admin_secret, admin_signer) = Self::generate_keypair();

        let mut cert = IdentityCertificate {
            identity_id: Uuid::new_v4(),
            subject_type: SubjectType::Admin,
            subject_id,
            issuer_identity_id: Some(wilaya_identity_id),
            credential_id: Uuid::new_v4(),
            generation: 1,
            status: CredentialStatus::Active,
            public_key: admin_signer.public_key(),
            algorithm_version: SIGNATURE_VERSION_ED25519,
            not_after: None,
            package_sequence: None,
            signature: None,
        };
        let signature = wilaya_signer
            .sign_certificate(&cert)
            .map_err(|e| AppError::Internal(format!("ADMIN cert signing failed: {e}")))?;
        cert.signature = Some(self.wrap_signature(signature)?);
        cert.require_signed()?;

        let store = self.db.executor().identity_store();
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

    fn wrap_signature(&self, bytes: Vec<u8>) -> AppResult<Ed25519CertificateSignature> {
        Ed25519CertificateSignature::try_from(bytes)
            .map_err(|e| AppError::Internal(format!("Invalid certificate signature: {e}")))
    }

    /// Generate the offline WILAYA bootstrap request (B5).
    ///
    /// RFC 2026-08-04-node-identity-trust §3.6 — "Root signs fully offline":
    /// on-node keypair generation, the secret is persisted x25519-encrypted via
    /// `node_key_store` (it NEVER leaves the node), and an UNSIGNED certificate
    /// (the CSR) is returned for the operator to carry to the Authority Root.
    /// Nothing else is persisted. Only valid from `Uninitialized` (one-way
    /// transition guard): re-entry is rejected once a node key exists.
    ///
    /// The CSR is a transport artifact (serialized JSON), never stored.
    pub fn generate_wilaya_request(
        &self,
        subject_id: Uuid,
        node_key_store: &NodeKeyStore,
    ) -> AppResult<IdentityCertificate> {
        if node_key_store.exists() {
            return Err(AppError::BusinessLogic(BusinessLogicError::OperationNotPermitted {
                message: "A node signing key already exists; WILAYA bootstrap already began (state is no longer Uninitialized)".into(),
            }));
        }
        let (secret, signer) = Self::generate_keypair();
        node_key_store.write(&secret)?;
        let request = IdentityCertificate {
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
        Ok(request)
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
            return Err(AppError::BusinessLogic(BusinessLogicError::OperationNotPermitted {
                message: "finalize_wilaya_provision requires a WILAYA certificate".into(),
            }));
        }
        if signed_cert.status != CredentialStatus::Active {
            return Err(AppError::BusinessLogic(BusinessLogicError::OperationNotPermitted {
                message: "finalize_wilaya_provision requires an ACTIVE certificate".into(),
            }));
        }
        if signed_cert.issuer_identity_id.is_some() {
            return Err(AppError::BusinessLogic(BusinessLogicError::OperationNotPermitted {
                message: "WILAYA certificates MUST be issued by the offline Authority Root (issuer None)".into(),
            }));
        }

        let root_public_key = resolve_root_public_key()?;
        let signature = signed_cert
            .signature
            .ok_or_else(|| AppError::Internal("Root-signed certificate missing signature".into()))?;
        let verifier = crate::infrastructure::security::Ed25519SignatureVerifier;
        let root_signature_valid = verifier
            .verify_certificate(signed_cert, &root_public_key, &signature)
            .map_err(|e| AppError::Internal(format!("Root signature verification failed: {e}")))?;
        if !root_signature_valid {
            return Err(AppError::BusinessLogic(BusinessLogicError::OperationNotPermitted {
                message: "Certificate signature is not valid for the Authority Root".into(),
            }));
        }

        let node_secret = node_key_store.read()?;
        let node_signer = crate::infrastructure::security::Ed25519SigningProvider::new(node_secret);
        if signed_cert.public_key != node_signer.public_key() {
            return Err(AppError::BusinessLogic(BusinessLogicError::OperationNotPermitted {
                message: "Certificate public key does not match the node signing key".into(),
            }));
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
        Ok(FinalizeWilayaProvisionResult::Provisioned(signed_cert.clone()))
    }

    /// Issue the FIRST ADMIN key linked to the bootstrap operator (B5).
    ///
    /// Guards (one-way transitions):
    /// - an ACTIVE WILAYA identity MUST exist (the local issuer);
    /// - no ACTIVE ADMIN identity may exist for the subject yet.
    ///
    /// Links the `users` row for `subject_username`:
    /// - existing row → credential material is untouched (additive legacy window:
    ///   a pre-existing password hash stays valid; Challenge–Response is added);
    /// - missing row → created with an EMPTY password hash = identity-only
    ///   credential (password login is rejected; the `.adminkey` is the sole key).
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
            return Err(AppError::Validation(crate::errors::ValidationError::Required {
                field: "subject_username".into(),
            }));
        }
        let store = self.db.executor().identity_store();
        let wilaya = store
            .get_active_by_subject_type(SubjectType::Wilaya)?
            .ok_or_else(|| {
                AppError::BusinessLogic(BusinessLogicError::OperationNotPermitted {
                    message: "An ACTIVE WILAYA identity is required before issuing the first ADMIN key".into(),
                })
            })?;

        let subject_id = {
            let users = self.db.executor().users();
            match users.get_user_by_username(username)? {
                Some(existing) => {
                    let parsed = Uuid::parse_str(&existing.id).map_err(|e| {
                        AppError::Internal(format!(
                            "Linked user '{}' has an invalid id '{}': {e}",
                            username, existing.id
                        ))
                    })?;
                    if let Some(existing_admin) = store.get_active_by_subject(
                        SubjectType::Admin,
                        &parsed,
                    )? {
                        return Err(AppError::BusinessLogic(
                            BusinessLogicError::OperationNotPermitted {
                                message: format!(
                                    "An ACTIVE ADMIN identity already exists for subject {parsed} (credential {})",
                                    existing_admin.credential_id
                                ),
                            },
                        ));
                    }
                    parsed
                }
                None => {
                    let id = Uuid::new_v4();
                    let node_id = crate::infrastructure::security::NodeIdentityProvider::current_node_id(
                        &crate::infrastructure::security::SettingsNodeIdentityProvider::new(
                            self.db.executor(),
                        ),
                    )
                    .unwrap_or_else(|_| "WILAYA".to_string());
                    users.upsert_user(
                        &id.to_string(),
                        username,
                        "",
                        UserRole::Admin,
                        &node_id,
                        now,
                    )?;
                    id
                }
            }
        };

        let wilaya_secret = node_key_store.read()?;
        let wilaya_signer =
            crate::infrastructure::security::Ed25519SigningProvider::new(wilaya_secret);

        let (admin_secret, admin_signer) = Self::generate_keypair();

        let mut cert = IdentityCertificate {
            identity_id: Uuid::new_v4(),
            subject_type: SubjectType::Admin,
            subject_id,
            issuer_identity_id: Some(wilaya.identity_id),
            credential_id: Uuid::new_v4(),
            generation: 1,
            status: CredentialStatus::Active,
            public_key: admin_signer.public_key(),
            algorithm_version: SIGNATURE_VERSION_ED25519,
            not_after: None,
            package_sequence: None,
            signature: None,
        };
        let signature = wilaya_signer
            .sign_certificate(&cert)
            .map_err(|e| AppError::Internal(format!("ADMIN cert signing failed: {e}")))?;
        cert.signature = Some(self.wrap_signature(signature)?);
        cert.require_signed()?;

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
}
