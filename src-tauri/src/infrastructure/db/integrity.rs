use crate::errors::AppError;
use rusqlite::Connection;

/// Performs a low-level integrity check on a specific SQLite file path.
/// Note: This expects a plaintext SQLite file. For encrypted backups, decrypt first.
pub fn verify_file_integrity(path: &str) -> Result<String, AppError> {
    let conn = Connection::open(path)?;
    verify_connection_integrity(&conn)
}

/// Performs a low-level integrity check using an existing connection.
pub fn verify_connection_integrity(conn: &Connection) -> Result<String, AppError> {
    let integrity: String = conn.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
    Ok(integrity)
}
