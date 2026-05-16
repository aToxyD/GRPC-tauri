//! Backup Domain Logic
//!
//! This module contains pure logic and validation rules for backup operations.
//! The concrete implementation is delegated to the infrastructure layer via BackupPort.

use std::path::Path;

pub use crate::domain::ports::backup::{BackupInfo, BackupPort};

/// SECURITY: Validate backup file path to prevent path traversal attacks
///
/// Checks:
/// - Path does not contain traversal sequences (../ or ..\)
/// - Path is within allowed backup directory
/// - Path has valid .db extension
pub fn validate_backup_path(backup_path: &Path, allowed_dir: &Path) -> Result<(), String> {
    // Check for path traversal sequences
    let path_str = backup_path.to_string_lossy();
    if path_str.contains("..") || path_str.contains("..\\") {
        return Err("Path traversal detected: path contains '..'".to_string());
    }

    // Check for encoded traversal attempts
    if path_str.contains("%2e%2e") || path_str.contains("%2E%2E") {
        return Err("Path traversal detected: encoded '..'".to_string());
    }

    // Verify path has .db extension
    match backup_path.extension() {
        Some(ext) if ext == "db" => {}
        _ => return Err("Invalid backup file extension: must be .db".to_string()),
    }

    // Canonicalize both paths for safe comparison
    // Note: Paths must exist for canonicalization. If not, validation fails here.
    let canonical_backup = backup_path
        .canonicalize()
        .map_err(|e| format!("Failed to canonicalize backup path: {}", e))?;
    let canonical_allowed = allowed_dir
        .canonicalize()
        .map_err(|e| format!("Failed to canonicalize allowed dir: {}", e))?;

    // Ensure backup path starts with allowed directory
    if !canonical_backup.starts_with(&canonical_allowed) {
        return Err(format!(
            "Backup path escapes allowed directory: {} is not within {}",
            canonical_backup.display(),
            canonical_allowed.display()
        ));
    }

    Ok(())
}

/// تحويل ساعات إلى ثواني
pub const fn hours_to_secs(hours: u64) -> u64 {
    hours * 3600
}
