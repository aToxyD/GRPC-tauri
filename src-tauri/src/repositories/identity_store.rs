//! Identity Store repository.
//!
//! RFC 2026-08-04-node-identity-trust / ADR-0038 §1.
//!
//! SQL-only, row mapping only. The Identity Store is the single source of
//! truth for identity state. Public verification material is stored as an
//! opaque BLOB; all cryptographic operations live in
//! `infrastructure/security/identity` (never in repositories).

use crate::domain::identity::{IdentityCertificate, IdentityStorePort, SubjectType};
use crate::errors::{AppError, AppResult};
use crate::repositories::executor::DbExecutor;
use rusqlite::{params, Row};
use uuid::Uuid;

/// Repository for the Identity Store table.
pub struct IdentityStoreRepository<'a> {
    executor: DbExecutor<'a>,
}

fn parse_uuid_field(value: String, idx: usize) -> rusqlite::Result<Uuid> {
    Uuid::parse_str(&value).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(idx, rusqlite::types::Type::Text, Box::new(e))
    })
}

fn parse_subject_type(value: String, idx: usize) -> rusqlite::Result<SubjectType> {
    value.parse().map_err(|e: String| {
        rusqlite::Error::FromSqlConversionFailure(
            idx,
            rusqlite::types::Type::Text,
            Box::new(AppError::Internal(e)),
        )
    })
}

fn parse_not_after(value: Option<String>) -> AppResult<Option<chrono::DateTime<chrono::Utc>>> {
    value
        .map(|s| crate::errors::parse_datetime_rfc3339(&s))
        .transpose()
}

fn map_row(row: &Row<'_>) -> rusqlite::Result<IdentityCertificate> {
    let identity_id: String = row.get(0)?;
    let subject_type: String = row.get(1)?;
    let subject_id: String = row.get(2)?;
    let issuer_identity_id: Option<String> = row.get(3)?;
    let credential_id: String = row.get(4)?;
    let generation: i64 = row.get(5)?;
    let status: String = row.get(6)?;
    let public_key: Vec<u8> = row.get(7)?;
    let algorithm_version: i64 = row.get(8)?;
    let not_after: Option<String> = row.get(9)?;
    let signature: Option<Vec<u8>> = row.get(10)?;

    let not_after = parse_not_after(not_after).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(9, rusqlite::types::Type::Text, Box::new(e))
    })?;

    let status = status
        .parse::<crate::domain::identity::CredentialStatus>()
        .map_err(|e: String| {
            rusqlite::Error::FromSqlConversionFailure(
                6,
                rusqlite::types::Type::Text,
                Box::new(AppError::Internal(e)),
            )
        })?;

    // NULL is a migration-only state (ADR-0039 §6); the fixed 64-byte length is
    // enforced at the domain boundary. Storage keeps raw bytes only.
    let signature = signature
        .map(|bytes| {
            bytes.try_into().map_err(|e: String| {
                rusqlite::Error::FromSqlConversionFailure(
                    10,
                    rusqlite::types::Type::Blob,
                    Box::new(AppError::Internal(e)),
                )
            })
        })
        .transpose()?;

    Ok(IdentityCertificate {
        identity_id: parse_uuid_field(identity_id, 0)?,
        subject_type: parse_subject_type(subject_type, 1)?,
        subject_id: parse_uuid_field(subject_id, 2)?,
        issuer_identity_id: issuer_identity_id
            .map(parse_uuid_field_with_idx(3))
            .transpose()?,
        credential_id: parse_uuid_field(credential_id, 4)?,
        generation: generation as u64,
        status,
        public_key,
        algorithm_version: algorithm_version as u16,
        not_after,
        // SEC-051 removed the identity_store.package_sequence column. The domain
        // field is retained (transport/signature-aware certificates set it), but
        // no value is persisted on the Identity Store, so it is read back as None.
        package_sequence: None,
        signature,
    })
}

fn parse_uuid_field_with_idx(idx: usize) -> impl FnOnce(String) -> rusqlite::Result<Uuid> {
    move |v: String| parse_uuid_field(v, idx)
}

const SELECT_COLUMNS: &str =
    "identity_id, subject_type, subject_id, issuer_identity_id, credential_id, generation, status, public_key, algorithm_version, not_after, signature";

impl<'a> IdentityStoreRepository<'a> {
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }
}

impl<'a> IdentityStorePort for IdentityStoreRepository<'a> {
    fn get_by_identity_id(&self, identity_id: &Uuid) -> AppResult<Option<IdentityCertificate>> {
        let result = self.executor.query_row_optional(
            &format!(
                "SELECT {SELECT_COLUMNS} FROM identity_store WHERE identity_id = ?1 AND deleted = 0"
            ),
            [identity_id.to_string()],
            map_row,
        )?;
        Ok(result)
    }

    fn get_active_by_subject(
        &self,
        subject_type: SubjectType,
        subject_id: &Uuid,
    ) -> AppResult<Option<IdentityCertificate>> {
        let result = self.executor.query_row_optional(
            &format!(
                "SELECT {SELECT_COLUMNS} FROM identity_store \
                 WHERE subject_type = ?1 AND subject_id = ?2 AND status = 'ACTIVE' AND deleted = 0 \
                 ORDER BY generation DESC LIMIT 1"
            ),
            params![subject_type.as_str(), subject_id.to_string()],
            map_row,
        )?;
        Ok(result)
    }

    fn get_active_by_subject_type(
        &self,
        subject_type: SubjectType,
    ) -> AppResult<Option<IdentityCertificate>> {
        let result = self.executor.query_row_optional(
            &format!(
                "SELECT {SELECT_COLUMNS} FROM identity_store \
                 WHERE subject_type = ?1 AND status = 'ACTIVE' AND deleted = 0 \
                 ORDER BY generation DESC LIMIT 1"
            ),
            [subject_type.as_str()],
            map_row,
        )?;
        Ok(result)
    }

    fn get_active_by_credential_id(
        &self,
        credential_id: &Uuid,
    ) -> AppResult<Option<IdentityCertificate>> {
        let result = self.executor.query_row_optional(
            &format!(
                "SELECT {SELECT_COLUMNS} FROM identity_store \
                 WHERE credential_id = ?1 AND status = 'ACTIVE' AND deleted = 0 \
                 ORDER BY generation DESC LIMIT 1"
            ),
            [credential_id.to_string()],
            map_row,
        )?;
        Ok(result)
    }

    fn list_all(&self) -> AppResult<Vec<IdentityCertificate>> {
        Ok(self.executor.query_all(
            &format!(
                "SELECT {SELECT_COLUMNS} FROM identity_store WHERE deleted = 0 \
                 ORDER BY subject_type, subject_id, generation"
            ),
            [],
            map_row,
        )?)
    }

    fn max_generation_for_credential(&self, credential_id: &Uuid) -> AppResult<Option<u64>> {
        let generation: Option<Option<i64>> = self.executor.query_row_optional(
            "SELECT MAX(generation) FROM identity_store WHERE credential_id = ?1 AND deleted = 0",
            [credential_id.to_string()],
            |row| row.get(0),
        )?;
        Ok(generation.and_then(|g| g.and_then(|v| u64::try_from(v).ok())))
    }

    fn upsert(&self, certificate: &IdentityCertificate, now: &str) -> AppResult<()> {
        self.executor.execute(
            "INSERT INTO identity_store \
             (identity_id, subject_type, subject_id, issuer_identity_id, credential_id, \
              generation, status, public_key, algorithm_version, not_after, \
              signature, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13) \
             ON CONFLICT(identity_id) DO UPDATE SET \
                 subject_type = excluded.subject_type, \
                 subject_id = excluded.subject_id, \
                 issuer_identity_id = excluded.issuer_identity_id, \
                 credential_id = excluded.credential_id, \
                 generation = excluded.generation, \
                 status = excluded.status, \
                 public_key = excluded.public_key, \
                 algorithm_version = excluded.algorithm_version, \
                 not_after = excluded.not_after, \
                 signature = excluded.signature, \
                 updated_at = excluded.updated_at, \
                 deleted = 0",
            params![
                certificate.identity_id.to_string(),
                certificate.subject_type.as_str(),
                certificate.subject_id.to_string(),
                certificate.issuer_identity_id.map(|id| id.to_string()),
                certificate.credential_id.to_string(),
                certificate.generation as i64,
                certificate.status.as_str(),
                certificate.public_key,
                certificate.algorithm_version as i64,
                certificate.not_after.map(|dt| dt.to_rfc3339()),
                certificate.signature.as_ref().map(|sig| sig.to_vec()),
                now,
                now,
            ],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::ConnectionFactory;
    use crate::db::Database;
    use crate::domain::identity::{CredentialStatus, IdentityStorePort};

    fn make_executor(db: &Database) -> DbExecutor<'_> {
        db.executor()
    }

    fn sample_certificate(identity_id: Uuid) -> IdentityCertificate {
        IdentityCertificate {
            identity_id,
            subject_type: SubjectType::Wilaya,
            subject_id: identity_id,
            issuer_identity_id: None,
            credential_id: Uuid::new_v4(),
            generation: 1,
            status: CredentialStatus::Active,
            public_key: vec![7u8; 32],
            // Ed25519 (RFC 8032): signature_version / algorithm_version = 2 (ADR-0038).
            algorithm_version: 2,
            not_after: None,
            package_sequence: Some(1),
            // NULL signature = legacy migration record only (ADR-0039 §6).
            signature: None,
        }
    }

    #[test]
    fn signature_column_roundtrip() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let repo = IdentityStoreRepository::new(make_executor(&db));

        let mut cert = sample_certificate(Uuid::new_v4());
        let signature = crate::domain::identity::test_signature([3u8; 64]);
        cert.signature = Some(signature);
        repo.upsert(&cert, "2026-08-04T00:00:00Z").unwrap();

        let stored = repo
            .get_by_identity_id(&cert.identity_id)
            .unwrap()
            .expect("row present");
        assert_eq!(stored.signature, Some(signature));

        // Legacy NULL round-trips as None.
        let legacy = sample_certificate(Uuid::new_v4());
        repo.upsert(&legacy, "2026-08-04T00:00:00Z").unwrap();
        let stored = repo
            .get_by_identity_id(&legacy.identity_id)
            .unwrap()
            .expect("row present");
        assert_eq!(stored.signature, None);
    }

    #[test]
    fn insert_and_read_by_identity_id() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let repo = IdentityStoreRepository::new(make_executor(&db));

        let id = Uuid::new_v4();
        repo.upsert(&sample_certificate(id), "2026-08-04T00:00:00Z")
            .unwrap();

        let cert = repo.get_by_identity_id(&id).unwrap().expect("row present");
        assert_eq!(cert.identity_id, id);
        assert_eq!(cert.subject_type, SubjectType::Wilaya);
        assert_eq!(cert.algorithm_version, 2);
        assert_eq!(cert.public_key, vec![7u8; 32]);
        assert_eq!(cert.generation, 1);
    }

    #[test]
    fn active_by_subject_type_resolves_wilaya_singleton() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let repo = IdentityStoreRepository::new(make_executor(&db));

        let id = Uuid::new_v4();
        repo.upsert(&sample_certificate(id), "2026-08-04T00:00:00Z")
            .unwrap();

        let wilaya = repo
            .get_active_by_subject_type(SubjectType::Wilaya)
            .unwrap()
            .expect("wilaya identity present");
        assert_eq!(wilaya.identity_id, id);

        let unit = repo.get_active_by_subject_type(SubjectType::Unit).unwrap();
        assert!(unit.is_none());
    }

    #[test]
    fn active_by_credential_id_and_subject() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let repo = IdentityStoreRepository::new(make_executor(&db));

        let cert = sample_certificate(Uuid::new_v4());
        repo.upsert(&cert, "2026-08-04T00:00:00Z").unwrap();

        let by_cred = repo
            .get_active_by_credential_id(&cert.credential_id)
            .unwrap()
            .expect("credential bound");
        assert_eq!(by_cred.identity_id, cert.identity_id);

        let by_subject = repo
            .get_active_by_subject(cert.subject_type, &cert.subject_id)
            .unwrap()
            .expect("subject bound");
        assert_eq!(by_subject.identity_id, cert.identity_id);
    }

    #[test]
    fn upsert_updates_existing_identity_id() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let repo = IdentityStoreRepository::new(make_executor(&db));

        let id = Uuid::new_v4();
        let mut cert = sample_certificate(id);
        repo.upsert(&cert, "2026-08-04T00:00:00Z").unwrap();

        cert.generation = 2;
        cert.status = CredentialStatus::Superseded;
        repo.upsert(&cert, "2026-08-04T01:00:00Z").unwrap();

        let stored = repo.get_by_identity_id(&id).unwrap().unwrap();
        assert_eq!(stored.generation, 2);
        assert_eq!(stored.status, CredentialStatus::Superseded);
    }

    #[test]
    fn max_generation_tracks_full_credential_history() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let repo = IdentityStoreRepository::new(make_executor(&db));

        let credential_id = Uuid::new_v4();
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();

        // Gen 1 promoted ACTIVE, then superseded by gen 2 (Invariant 6).
        let mut gen1 = sample_certificate(id1);
        gen1.credential_id = credential_id;
        repo.upsert(&gen1, "2026-08-04T00:00:00Z").unwrap();
        gen1.status = CredentialStatus::Superseded;
        repo.upsert(&gen1, "2026-08-04T01:00:00Z").unwrap();

        let mut gen2 = sample_certificate(id2);
        gen2.credential_id = credential_id;
        gen2.generation = 2;
        repo.upsert(&gen2, "2026-08-04T02:00:00Z").unwrap();

        assert_eq!(
            repo.max_generation_for_credential(&credential_id).unwrap(),
            Some(2)
        );

        let unknown = Uuid::new_v4();
        assert_eq!(repo.max_generation_for_credential(&unknown).unwrap(), None);
    }

    #[test]
    fn unique_active_per_subject_is_enforced() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let repo = IdentityStoreRepository::new(make_executor(&db));

        let subject_id = Uuid::new_v4();
        let first = IdentityCertificate {
            identity_id: Uuid::new_v4(),
            subject_id,
            ..sample_certificate(Uuid::new_v4())
        };
        repo.upsert(&first, "2026-08-04T00:00:00Z").unwrap();

        let second = IdentityCertificate {
            identity_id: Uuid::new_v4(),
            subject_id,
            credential_id: Uuid::new_v4(),
            generation: 2,
            ..first.clone()
        };
        let err = repo.upsert(&second, "2026-08-04T01:00:00Z").unwrap_err();
        assert!(
            matches!(err, AppError::Sqlite(_)),
            "second ACTIVE for same subject must violate the unique index, got {err:?}"
        );
    }
}
