//! Identity Store persistence port.
//!
//! RFC 2026-08-04-node-identity-trust §3.1 / ADR-0038 §1.
//!
//! The Identity Store is the single source of truth for identity state
//! (Invariant 9). Implemented by `repositories::identity_store::IdentityStoreRepository`.

use crate::domain::identity::{IdentityCertificate, SubjectType};
use crate::errors::AppResult;

/// Contract owner: domain identity (ADR-0038 §1). Consumers read projections
/// through this port; no consumer may redefine the identity model.
pub trait IdentityStorePort {
    /// Read a certificate by its immutable identity identifier.
    fn get_by_identity_id(
        &self,
        identity_id: &uuid::Uuid,
    ) -> AppResult<Option<IdentityCertificate>>;

    /// Read the single ACTIVE certificate for a subject (Invariant 6).
    fn get_active_by_subject(
        &self,
        subject_type: SubjectType,
        subject_id: &uuid::Uuid,
    ) -> AppResult<Option<IdentityCertificate>>;

    /// Read the ACTIVE certificate of a singleton subject type (e.g. the
    /// local WILAYA). Meaningful only when exactly one such subject exists.
    fn get_active_by_subject_type(
        &self,
        subject_type: SubjectType,
    ) -> AppResult<Option<IdentityCertificate>>;

    /// Read the ACTIVE certificate currently bound to a credential identifier.
    fn get_active_by_credential_id(
        &self,
        credential_id: &uuid::Uuid,
    ) -> AppResult<Option<IdentityCertificate>>;

    /// Highest generation ever persisted for a credential (across statuses).
    ///
    /// Credential Guard input (RFC 2026-08-04 §3.4.2): generation is strictly
    /// monotonic per credential, so the max over the full history — not just
    /// ACTIVE rows — is the authoritative stored generation.
    fn max_generation_for_credential(&self, credential_id: &uuid::Uuid) -> AppResult<Option<u64>>;

    /// List all persisted certificates (audit / verification).
    fn list_all(&self) -> AppResult<Vec<IdentityCertificate>>;

    /// Insert or replace the certificate row for `identity_id`.
    ///
    /// Raw row write used by bootstrap and lifecycle services. The
    /// `(subject_type, subject_id)` ACTIVE unique index enforces Invariant 6:
    /// promoting a new ACTIVE credential requires superseding the previous one
    /// in the same transaction.
    fn upsert(&self, certificate: &IdentityCertificate, now: &str) -> AppResult<()>;
}
