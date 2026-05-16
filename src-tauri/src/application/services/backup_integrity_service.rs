use crate::errors::AppError;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Serialize, Deserialize)]
pub struct BackupIntegrityResult {
    pub valid: bool,
    pub integrity_score: u8,
    pub warnings: Vec<String>,
    pub failures: Vec<String>,
}

pub struct BackupIntegrityService;
impl BackupIntegrityService {
    pub fn verify_backup_file(
        path: &Path,
        backup_port: &dyn crate::domain::ports::backup::BackupPort,
    ) -> Result<BackupIntegrityResult, AppError> {
        let mut failures = Vec::new();
        let mut score = 100u8;

        match backup_port.verify_backup_integrity(path) {
            Ok(status) => {
                if status != "ok" {
                    failures.push(format!("integrity_check_failed: {}", status));
                    score = 20;
                }
            }
            Err(e) => {
                log::error!(target: "grpc::backup", "Failed to verify backup integrity for {}: {}", path.display(), e);
                failures.push(format!("sqlite_open_failed: {}", e));
                score = 0;
            }
        }

        Ok(BackupIntegrityResult {
            valid: failures.is_empty(),
            integrity_score: score,
            warnings: Vec::new(),
            failures,
        })
    }
}
