use crate::errors::AppError;
use std::path::Path;

/// Read the maximum archived year from a detached SQLite file (e.g. a backup).
/// This is used during restore to ensure we don't regress archived state.
/// Note: This expects a plaintext SQLite file. For encrypted backups, decrypt first.
pub fn get_max_archived_year_in_file(path: &Path) -> Result<Option<i32>, AppError> {
    let conn =
        rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|e| {
                AppError::Internal(format!("cannot open file for archived check: {}", e))
            })?;

    get_max_archived_year_in_conn(&conn)
}

/// Read the maximum archived year from an existing SQLite connection.
pub fn get_max_archived_year_in_conn(conn: &rusqlite::Connection) -> Result<Option<i32>, AppError> {
    let max: Option<i32> = conn
        .query_row(
            "SELECT MAX(year) FROM fiscal_year_status WHERE archived = 1",
            [],
            |r| r.get::<_, Option<i32>>(0),
        )
        .map_err(|e| AppError::Internal(format!("file archived year query: {}", e)))?;
    Ok(max)
}
