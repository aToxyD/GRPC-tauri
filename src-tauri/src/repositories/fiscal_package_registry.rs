use super::DbExecutor;
use rusqlite::{params, Result as SqliteResult};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FiscalPackageRegistryEntry {
    pub id: i64,
    pub transition_id: String,
    pub fiscal_year: i32,
    pub next_year: i32,
    pub package_fingerprint: String,
    pub signing_key_id: String,
    pub exported_at: String,
    pub applied_at: Option<String>,
    pub retention_status: String,
    pub exported_by: String,
    pub applied_by: Option<String>,
    pub archived: bool,
    pub notes: Option<String>,
}

pub struct FiscalPackageRegistryRepository<'a> {
    executor: DbExecutor<'a>,
}

impl<'a> FiscalPackageRegistryRepository<'a> {
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }

    pub fn insert(&self, entry: &FiscalPackageRegistryEntry) -> SqliteResult<()> {
        self.executor.execute(
            "INSERT INTO fiscal_closure_package_registry (
                transition_id, fiscal_year, next_year, package_fingerprint,
                signing_key_id, exported_at, applied_at, retention_status,
                exported_by, applied_by, archived, notes
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                entry.transition_id,
                entry.fiscal_year,
                entry.next_year,
                entry.package_fingerprint,
                entry.signing_key_id,
                entry.exported_at,
                entry.applied_at,
                entry.retention_status,
                entry.exported_by,
                entry.applied_by,
                if entry.archived { 1 } else { 0 },
                entry.notes
            ],
        )?;
        Ok(())
    }

    pub fn upsert(&self, entry: &FiscalPackageRegistryEntry) -> SqliteResult<()> {
        self.executor.execute(
            "INSERT INTO fiscal_closure_package_registry (
                transition_id, fiscal_year, next_year, package_fingerprint,
                signing_key_id, exported_at, applied_at, retention_status,
                exported_by, applied_by, archived, notes
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
            ON CONFLICT(transition_id) DO UPDATE SET
                package_fingerprint = excluded.package_fingerprint,
                signing_key_id = excluded.signing_key_id,
                exported_at = excluded.exported_at,
                retention_status = excluded.retention_status,
                exported_by = excluded.exported_by,
                notes = excluded.notes",
            params![
                entry.transition_id,
                entry.fiscal_year,
                entry.next_year,
                entry.package_fingerprint,
                entry.signing_key_id,
                entry.exported_at,
                entry.applied_at,
                entry.retention_status,
                entry.exported_by,
                entry.applied_by,
                if entry.archived { 1 } else { 0 },
                entry.notes
            ],
        )?;
        Ok(())
    }

    pub fn mark_applied(&self, transition_id: &str, applied_by: &str) -> SqliteResult<()> {
        self.executor.execute(
            "UPDATE fiscal_closure_package_registry SET applied_at = ?1, applied_by = ?2 WHERE transition_id = ?3",
            params![chrono::Utc::now().to_rfc3339(), applied_by, transition_id],
        )?;
        Ok(())
    }

    pub fn update_retention_status(&self, transition_id: &str, status: &str) -> SqliteResult<()> {
        self.executor.execute(
            "UPDATE fiscal_closure_package_registry SET retention_status = ?1 WHERE transition_id = ?2",
            params![status, transition_id],
        )?;
        Ok(())
    }

    pub fn mark_archived(&self, transition_id: &str, archived: bool) -> SqliteResult<()> {
        self.executor.execute(
            "UPDATE fiscal_closure_package_registry SET archived = ?1 WHERE transition_id = ?2",
            params![if archived { 1 } else { 0 }, transition_id],
        )?;
        Ok(())
    }

    pub fn list_all(&self) -> SqliteResult<Vec<FiscalPackageRegistryEntry>> {
        let mut stmt = self.executor.prepare(
            "SELECT id, transition_id, fiscal_year, next_year, package_fingerprint,
                    signing_key_id, exported_at, applied_at, retention_status,
                    exported_by, applied_by, archived, notes
             FROM fiscal_closure_package_registry
             ORDER BY exported_at DESC",
        )?;

        let entries = stmt.query_map([], |row| {
            Ok(FiscalPackageRegistryEntry {
                id: row.get(0)?,
                transition_id: row.get(1)?,
                fiscal_year: row.get(2)?,
                next_year: row.get(3)?,
                package_fingerprint: row.get(4)?,
                signing_key_id: row.get(5)?,
                exported_at: row.get(6)?,
                applied_at: row.get(7)?,
                retention_status: row.get(8)?,
                exported_by: row.get(9)?,
                applied_by: row.get(10)?,
                archived: row.get::<_, i32>(11)? != 0,
                notes: row.get(12)?,
            })
        })?;

        let mut result = Vec::new();
        for entry in entries {
            result.push(entry?);
        }
        Ok(result)
    }
}
