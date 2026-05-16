use std::io;
use std::path::{Path, PathBuf};

/// Backup information
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BackupInfo {
    pub filename: String,
    pub path: String,
    pub size_bytes: u64,
    pub created: String,
}

/// Port for backup operations
pub trait BackupPort: Send + Sync {
    /// Create a backup from a live database connection
    fn create_backup_from_conn(&self, src: &rusqlite::Connection) -> io::Result<PathBuf>;

    /// Create a backup from the database file
    fn create_backup(&self) -> io::Result<PathBuf>;

    /// Check if a backup is needed based on the interval
    fn should_backup(&self) -> bool;

    /// Create a backup if needed
    fn backup_if_needed(&self) -> io::Result<Option<PathBuf>>;

    /// List all backups
    fn get_backup_info(&self) -> io::Result<Vec<BackupInfo>>;

    /// Restore from a backup atomically
    fn restore_backup_atomic(&self, backup_path: &Path) -> io::Result<()>;

    /// Verify integrity of an encrypted backup file.
    /// This will decrypt the file to a temporary location first.
    fn verify_backup_integrity(&self, backup_path: &Path) -> io::Result<String>;

    /// Get the maximum archived year from an encrypted backup file.
    fn get_backup_max_archived_year(&self, backup_path: &Path) -> io::Result<Option<i32>>;
}
