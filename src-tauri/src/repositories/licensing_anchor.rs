//! Licensing trust anchor storage (SQL only).
//!
//! ADR-0042 §2 / artifact-spec v1 §9 (provisioning-v1).
//!
//! Stores the consumer's DERIVED view of the single active licensing Trust
//! Anchor (public key only — grants no operational trust). Exactly one active
//! anchor per installation (ADR-0005 Invariant 6); the storage layer enforces
//! it via a partial unique index, and the repository exposes the deactivate +
//! insert primitives the service composes inside a transaction for rotation
//! (replacing the active anchor).

use crate::errors::{AppError, AppResult, ValidationError};

use super::executor::DbExecutor;

/// A stored licensing trust anchor row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LicensingAnchorRow {
    pub key_id: String,
    /// Raw 32-byte Ed25519 public key.
    pub public_key: Vec<u8>,
    pub algorithm: String,
    pub installed_at: String,
    pub is_active: bool,
}

pub struct LicensingAnchorRepository<'a> {
    executor: DbExecutor<'a>,
}

impl<'a> LicensingAnchorRepository<'a> {
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }

    /// The single active anchor, if installed.
    pub fn get_active(&self) -> AppResult<Option<LicensingAnchorRow>> {
        self.executor
            .query_row_optional(
                "SELECT key_id, public_key, algorithm, installed_at, is_active \
                 FROM licensing_anchor WHERE is_active = 1",
                [],
                map_row,
            )
            .map_err(AppError::from)
    }

    /// Whether an active anchor exists (the licensing gate trigger).
    pub fn has_active(&self) -> AppResult<bool> {
        Ok(self.get_active()?.is_some())
    }

    /// All stored anchors (active + retired), most recently installed first.
    pub fn list_all(&self) -> AppResult<Vec<LicensingAnchorRow>> {
        self.executor
            .query_all(
                "SELECT key_id, public_key, algorithm, installed_at, is_active \
                 FROM licensing_anchor ORDER BY installed_at DESC",
                [],
                map_row,
            )
            .map_err(AppError::from)
    }

    /// Retire every anchor (`is_active = 0`). Composed with
    /// [`Self::insert_active`] inside a service transaction to rotate the
    /// single active anchor.
    pub fn deactivate_all(&self) -> AppResult<()> {
        self.executor
            .execute("UPDATE licensing_anchor SET is_active = 0", [])
            .map_err(AppError::from)?;
        Ok(())
    }

    /// Insert a new anchor as the single active one. Must be preceded by
    /// [`Self::deactivate_all`] under rotation (single-active invariant).
    pub fn insert_active(
        &self,
        key_id: &str,
        public_key: &[u8],
        algorithm: &str,
        installed_at: &str,
    ) -> AppResult<()> {
        validate_key_id(key_id)?;
        if public_key.len() != 32 {
            return Err(AppError::Validation(ValidationError::InvalidFormat {
                field: "public_key".into(),
                message: "anchor public key must be exactly 32 bytes".into(),
            }));
        }
        self.executor
            .execute(
                "INSERT INTO licensing_anchor (key_id, public_key, algorithm, installed_at, is_active) \
                 VALUES (?1, ?2, ?3, ?4, 1)",
                rusqlite::params![key_id, public_key, algorithm, installed_at],
            )
            .map_err(AppError::from)?;
        Ok(())
    }
}

fn map_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<LicensingAnchorRow> {
    Ok(LicensingAnchorRow {
        key_id: row.get(0)?,
        public_key: row.get(1)?,
        algorithm: row.get(2)?,
        installed_at: row.get(3)?,
        is_active: row.get::<_, i64>(4)? != 0,
    })
}

fn validate_key_id(key_id: &str) -> AppResult<()> {
    if key_id.trim().is_empty() {
        return Err(AppError::Validation(ValidationError::InvalidFormat {
            field: "key_id".into(),
            message: "anchor key_id is empty".into(),
        }));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{ConnectionFactory, Database};
    use crate::infrastructure::licensing::signing::LICENSE_SIGNING_ALGORITHM;

    fn repo(db: &Database) -> LicensingAnchorRepository<'_> {
        LicensingAnchorRepository::new(db.executor())
    }

    #[test]
    fn fresh_db_has_no_active_anchor() {
        let db = ConnectionFactory::new_for_test().unwrap();
        assert_eq!(repo(&db).get_active().unwrap(), None);
        assert!(!repo(&db).has_active().unwrap());
    }

    #[test]
    fn insert_active_then_read() {
        let db = ConnectionFactory::new_for_test().unwrap();
        repo(&db)
            .insert_active(
                "lk-2026-0001",
                &[7u8; 32],
                LICENSE_SIGNING_ALGORITHM,
                "2026-08-09T00:00:00Z",
            )
            .unwrap();
        let row = repo(&db).get_active().unwrap().unwrap();
        assert_eq!(row.key_id, "lk-2026-0001");
        assert_eq!(row.public_key, vec![7u8; 32]);
        assert!(row.is_active);
        assert!(repo(&db).has_active().unwrap());
    }

    #[test]
    fn rotation_replaces_the_single_active_anchor() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let r = repo(&db);
        r.insert_active(
            "lk-old",
            &[1u8; 32],
            LICENSE_SIGNING_ALGORITHM,
            "2026-08-09T00:00:00Z",
        )
        .unwrap();
        // Partial unique index forbids a second active anchor → insert fails.
        assert!(r
            .insert_active(
                "lk-new",
                &[2u8; 32],
                LICENSE_SIGNING_ALGORITHM,
                "2026-08-09T00:00:01Z"
            )
            .is_err());
        // Deactivate-all + insert is the legal rotation path.
        r.deactivate_all().unwrap();
        r.insert_active(
            "lk-new",
            &[2u8; 32],
            LICENSE_SIGNING_ALGORITHM,
            "2026-08-09T00:00:01Z",
        )
        .unwrap();
        r.deactivate_all().unwrap();
        r.insert_active(
            "lk-second",
            &[3u8; 32],
            LICENSE_SIGNING_ALGORITHM,
            "2026-08-09T00:00:02Z",
        )
        .unwrap();
        let active = r.get_active().unwrap().unwrap();
        assert_eq!(active.key_id, "lk-second");
        assert_eq!(r.list_all().unwrap().len(), 3);
        // Only one active row remains.
        let actives: Vec<_> = r
            .list_all()
            .unwrap()
            .into_iter()
            .filter(|a| a.is_active)
            .collect();
        assert_eq!(actives.len(), 1);
    }

    #[test]
    fn rejects_wrong_key_length() {
        let db = ConnectionFactory::new_for_test().unwrap();
        assert!(repo(&db)
            .insert_active("k", &[0u8; 16], LICENSE_SIGNING_ALGORITHM, "now")
            .is_err());
        assert!(repo(&db)
            .insert_active("k", &[], LICENSE_SIGNING_ALGORITHM, "now")
            .is_err());
    }

    #[test]
    fn rejects_empty_key_id() {
        let db = ConnectionFactory::new_for_test().unwrap();
        assert!(repo(&db)
            .insert_active("", &[0u8; 32], LICENSE_SIGNING_ALGORITHM, "now")
            .is_err());
        assert!(repo(&db)
            .insert_active("   ", &[0u8; 32], LICENSE_SIGNING_ALGORITHM, "now")
            .is_err());
    }
}
