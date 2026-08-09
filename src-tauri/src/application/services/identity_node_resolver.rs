//! Local node identity signer resolver.
//!
//! RFC 2026-08-04-node-identity-trust §3.12 / ADR-0038.
//!
//! `NodeIdentityResolver` is the SINGLE source of truth for resolving the local
//! node's Ed25519 signing key paired with its ACTIVE certificate. It applies
//! R5 (the derived public key MUST match the certificate's public key) at every
//! issuance and installation point, fail-closed. The Unit CSR pipeline
//! (`sign_unit_identity_request`, `finalize_unit_provision`) and the generalized
//! ADMIN issuance path all route through this resolver.

use crate::db::Database;
use crate::domain::identity::{
    CredentialStatus, IdentityCertificate, IdentitySigner, IdentityStorePort, SubjectType,
    SIGNATURE_VERSION_ED25519,
};
use crate::errors::{AppError, AppResult, BusinessLogicError};
use crate::infrastructure::identity::NodeKeyStore;
use crate::infrastructure::security::Ed25519SigningProvider;
use crate::repositories::RepositoryProvider;

/// A local node identity whose signing key is proven to match its certificate.
pub struct ResolvedNodeIdentity {
    pub certificate: IdentityCertificate,
    pub signer: Ed25519SigningProvider,
}

impl std::fmt::Debug for ResolvedNodeIdentity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResolvedNodeIdentity")
            .field("certificate", &self.certificate)
            .field("signer", &"Ed25519SigningProvider")
            .finish()
    }
}

/// Pure resolver — no persistence, no identity state mutation.
pub struct NodeIdentityResolver;

impl NodeIdentityResolver {
    /// Derive the Ed25519 public key for a raw 32-byte secret (RFC 8032).
    ///
    /// Separated as a pure helper so the R5 matrix is testable without touching
    /// the Identity Store.
    pub fn derive_public_key(secret: &[u8; 32]) -> Vec<u8> {
        Ed25519SigningProvider::new(*secret).public_key()
    }

    /// Resolve the local signing key paired with the ACTIVE certificate of
    /// `subject_type`. Fail-closed chain:
    ///
    /// - no node key → `Ok(None)` (node not provisioned for this subject);
    /// - no ACTIVE certificate for the subject type → `Ok(None)`;
    /// - certificate not `Active` → error;
    /// - `algorithm_version` ≠ Ed25519 profile → error;
    /// - R5: derived public key ≠ certificate public key → error;
    /// - otherwise → `Ok(Some(ResolvedNodeIdentity))`.
    pub fn resolve_local_signer(
        db: &Database,
        node_key_store: &NodeKeyStore,
        subject_type: SubjectType,
    ) -> AppResult<Option<ResolvedNodeIdentity>> {
        let Some(secret) = node_key_store.read_if_exists()? else {
            return Ok(None);
        };
        let Some(certificate) = db
            .executor()
            .identity_store()
            .get_active_by_subject_type(subject_type)?
        else {
            return Ok(None);
        };
        if certificate.status != CredentialStatus::Active {
            return Err(Self::permitted(format!(
                "Local {subject_type} identity {} is not ACTIVE (status {})",
                certificate.identity_id, certificate.status
            )));
        }
        if certificate.algorithm_version != SIGNATURE_VERSION_ED25519 {
            return Err(Self::permitted(format!(
                "Local {subject_type} identity uses algorithm_version {}; expected Ed25519 ({SIGNATURE_VERSION_ED25519})",
                certificate.algorithm_version
            )));
        }
        let signer = Ed25519SigningProvider::new(secret);
        if signer.public_key() != certificate.public_key {
            return Err(Self::permitted(format!(
                "R5: local {subject_type} node signing key does not match the ACTIVE certificate public key"
            )));
        }
        Ok(Some(ResolvedNodeIdentity {
            certificate,
            signer,
        }))
    }

    fn permitted(message: String) -> AppError {
        AppError::BusinessLogic(BusinessLogicError::OperationNotPermitted { message })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::ConnectionFactory;
    use crate::infrastructure::identity::NodeKeyStore;
    use uuid::Uuid;

    const FIXED_NOW: &str = "2026-08-04T00:00:00Z";
    const SECRET: [u8; 32] = [42u8; 32];

    fn make_cert(subject_type: SubjectType, public_key: Vec<u8>) -> IdentityCertificate {
        IdentityCertificate {
            identity_id: Uuid::new_v4(),
            subject_type,
            subject_id: Uuid::new_v4(),
            issuer_identity_id: None,
            credential_id: Uuid::new_v4(),
            generation: 1,
            status: CredentialStatus::Active,
            public_key,
            algorithm_version: SIGNATURE_VERSION_ED25519,
            not_after: None,
            package_sequence: None,
            signature: None,
        }
    }

    fn store_with_secret(dir: &tempfile::TempDir) -> NodeKeyStore {
        let store = NodeKeyStore::new(dir.path().join("node"));
        store.write(&SECRET).unwrap();
        store
    }

    #[test]
    fn no_node_key_is_not_provisioned() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let store = NodeKeyStore::new(dir.path().join("node"));
        let resolved =
            NodeIdentityResolver::resolve_local_signer(&db, &store, SubjectType::Wilaya).unwrap();
        assert!(resolved.is_none());
    }

    #[test]
    fn node_key_without_active_cert_is_not_provisioned() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let store = store_with_secret(&dir);
        let resolved =
            NodeIdentityResolver::resolve_local_signer(&db, &store, SubjectType::Wilaya).unwrap();
        assert!(resolved.is_none());
    }

    #[test]
    fn non_active_certificate_is_not_provisioned() {
        // The store port already filters `get_active_by_subject_type` to ACTIVE
        // rows, so a non-ACTIVE certificate resolves as not-provisioned. The
        // in-code `status != Active` check remains as defense-in-depth against
        // port-contract drift.
        let db = ConnectionFactory::new_for_test().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let store = store_with_secret(&dir);
        let mut cert = make_cert(
            SubjectType::Wilaya,
            NodeIdentityResolver::derive_public_key(&SECRET),
        );
        cert.status = CredentialStatus::Revoked;
        db.executor()
            .identity_store()
            .upsert(&cert, FIXED_NOW)
            .unwrap();
        let resolved =
            NodeIdentityResolver::resolve_local_signer(&db, &store, SubjectType::Wilaya).unwrap();
        assert!(resolved.is_none());
    }

    #[test]
    fn wrong_algorithm_version_fails_closed() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let store = store_with_secret(&dir);
        let mut cert = make_cert(
            SubjectType::Wilaya,
            NodeIdentityResolver::derive_public_key(&SECRET),
        );
        cert.algorithm_version = SIGNATURE_VERSION_ED25519 + 1;
        db.executor()
            .identity_store()
            .upsert(&cert, FIXED_NOW)
            .unwrap();
        let err = NodeIdentityResolver::resolve_local_signer(&db, &store, SubjectType::Wilaya)
            .unwrap_err();
        assert!(err.to_string().contains("algorithm_version"), "got {err:?}");
    }

    #[test]
    fn public_key_mismatch_fails_closed_r5() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let store = store_with_secret(&dir);
        let cert = make_cert(SubjectType::Wilaya, vec![7u8; 32]);
        db.executor()
            .identity_store()
            .upsert(&cert, FIXED_NOW)
            .unwrap();
        let err = NodeIdentityResolver::resolve_local_signer(&db, &store, SubjectType::Wilaya)
            .unwrap_err();
        assert!(err.to_string().contains("does not match"), "got {err:?}");
    }

    #[test]
    fn happy_path_resolves_signer_matching_certificate() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let store = store_with_secret(&dir);
        let public_key = NodeIdentityResolver::derive_public_key(&SECRET);
        let cert = make_cert(SubjectType::Wilaya, public_key.clone());
        db.executor()
            .identity_store()
            .upsert(&cert, FIXED_NOW)
            .unwrap();
        let resolved = NodeIdentityResolver::resolve_local_signer(&db, &store, SubjectType::Wilaya)
            .unwrap()
            .expect("resolved");
        assert_eq!(resolved.certificate.public_key, public_key);
        assert_eq!(resolved.signer.public_key(), public_key);
        assert_eq!(resolved.certificate.identity_id, cert.identity_id);
    }
}
