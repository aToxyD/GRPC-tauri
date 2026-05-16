use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::errors::AppError;
use crate::repositories::DbExecutor;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportAuditEventProjection {
    pub occurred_at: DateTime<Utc>,
    pub package_id: String,
    pub package_kind: String,
    pub source_node_id: String,
    pub status: String,
    pub reason_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportAuditQuery {
    pub limit: u32,
    pub package_id: Option<String>,
    pub reason_code: Option<String>,
}

pub fn list_import_audit_events(
    executor: DbExecutor<'_>,
    query: &ImportAuditQuery,
) -> Result<Vec<ImportAuditEventProjection>, AppError> {
    let limit = query.limit.clamp(1, 200) as i64;
    Ok(executor.query_all(
        r#"SELECT occurred_at, package_id, package_kind,
                  COALESCE(source_node_id, '') as source_node_id,
                  CASE event_type
                    WHEN 'ImportSucceeded' THEN 'Succeeded'
                    WHEN 'ImportRejected' THEN 'Rejected'
                    ELSE 'Started'
                  END as status,
                  reason_code
           FROM import_audit_events
           WHERE (?1 IS NULL OR package_id = ?1)
             AND (?2 IS NULL OR reason_code = ?2)
           ORDER BY id DESC
           LIMIT ?3"#,
        rusqlite::params![
            query.package_id.as_deref(),
            query.reason_code.as_deref(),
            limit
        ],
        |row| {
            let occurred_at: String = row.get(0)?;
            let occurred_at = DateTime::parse_from_rfc3339(&occurred_at)
                .map(|dt| dt.with_timezone(&Utc))
                .or_else(|_| {
                    chrono::NaiveDateTime::parse_from_str(&occurred_at, "%Y-%m-%d %H:%M:%S")
                        .map(|naive| DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc))
                })
                .map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(
                        0,
                        rusqlite::types::Type::Text,
                        Box::new(e),
                    )
                })?;
            Ok(ImportAuditEventProjection {
                occurred_at,
                package_id: row.get(1)?,
                package_kind: row.get(2)?,
                source_node_id: row.get(3)?,
                status: row.get(4)?,
                reason_code: row.get(5)?,
            })
        },
    )?)
}
