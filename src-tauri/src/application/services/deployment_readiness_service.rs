//! Read-only deployment readiness verification — no auto-fix, no directory creation.

use crate::db::migrations::expected_schema_version;
use crate::errors::AppError;
use crate::infrastructure::backup::SqliteBackupAdapter;
use crate::infrastructure::logging::resolve_log_dir;
use crate::infrastructure::security::resolve_active_signing_key_id;
use crate::repositories::executor::DbExecutor;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DeploymentReadinessStatus {
    Ready,
    NotReady,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeploymentReadinessCheck {
    pub id: String,
    pub passed: bool,
    pub severity: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeploymentReadinessReport {
    pub status: DeploymentReadinessStatus,
    pub checks: Vec<DeploymentReadinessCheck>,
    pub blocking_failures: Vec<String>,
    pub warnings: Vec<String>,
}

pub struct DeploymentReadinessService<'a> {
    executor: DbExecutor<'a>,
    db_path: PathBuf,
}

impl<'a> DeploymentReadinessService<'a> {
    pub fn new(executor: DbExecutor<'a>, db_path: PathBuf) -> Self {
        Self { executor, db_path }
    }

    pub fn verify(&self) -> Result<DeploymentReadinessReport, AppError> {
        let mut checks = Vec::new();
        let mut blocking_failures = Vec::new();
        let mut warnings = Vec::new();

        self.check_database_accessibility(&mut checks, &mut blocking_failures);
        self.check_backup_directory(&mut checks, &mut blocking_failures, &mut warnings);
        self.check_logs_directory(&mut checks, &mut blocking_failures, &mut warnings);
        self.check_signing_key(&mut checks, &mut blocking_failures, &mut warnings);
        self.check_migration_consistency(&mut checks, &mut blocking_failures)?;
        self.check_single_open_fiscal_year(&mut checks, &mut blocking_failures)?;
        self.check_settings_current_year(&mut checks, &mut blocking_failures)?;
        self.check_archive_invariants(&mut checks, &mut blocking_failures, &mut warnings)?;
        self.check_restore_journal_leftovers(&mut checks, &mut blocking_failures);
        self.check_sqlite_integrity(&mut checks, &mut blocking_failures)?;

        let status = if blocking_failures.is_empty() {
            DeploymentReadinessStatus::Ready
        } else {
            DeploymentReadinessStatus::NotReady
        };

        Ok(DeploymentReadinessReport {
            status,
            checks,
            blocking_failures,
            warnings,
        })
    }

    fn push_check(
        checks: &mut Vec<DeploymentReadinessCheck>,
        blocking: &mut Vec<String>,
        warnings: &mut Vec<String>,
        id: &str,
        passed: bool,
        blocking_on_fail: bool,
        message: String,
    ) {
        let severity = if passed {
            "ok"
        } else if blocking_on_fail {
            "blocking"
        } else {
            "warning"
        };
        if !passed {
            if blocking_on_fail {
                blocking.push(format!("{}: {}", id, message));
            } else {
                warnings.push(format!("{}: {}", id, message));
            }
        }
        checks.push(DeploymentReadinessCheck {
            id: id.to_string(),
            passed,
            severity: severity.to_string(),
            message,
        });
    }

    fn check_database_accessibility(
        &self,
        checks: &mut Vec<DeploymentReadinessCheck>,
        blocking: &mut Vec<String>,
    ) {
        let exists = self.db_path.exists();
        let readable = exists && fs::File::open(&self.db_path).is_ok();
        let writable = exists
            && fs::OpenOptions::new()
                .append(true)
                .open(&self.db_path)
                .is_ok();
        let passed = exists && readable && writable;
        let msg = if !exists {
            format!("database file missing: {}", self.db_path.display())
        } else if !readable {
            "database file not readable".to_string()
        } else if !writable {
            "database file not writable".to_string()
        } else {
            format!("database accessible at {}", self.db_path.display())
        };
        Self::push_check(
            checks,
            blocking,
            &mut Vec::new(),
            "A_database_accessibility",
            passed,
            true,
            msg,
        );
    }

    fn check_backup_directory(
        &self,
        checks: &mut Vec<DeploymentReadinessCheck>,
        blocking: &mut Vec<String>,
        warnings: &mut Vec<String>,
    ) {
        let backup_dir = SqliteBackupAdapter::compute_backup_dir(&self.db_path);
        let exists = backup_dir.exists();
        let writable = exists
            && fs::OpenOptions::new()
                .write(true)
                .create(false)
                .open(backup_dir.join(".write_probe"))
                .is_ok()
            || probe_dir_writable(&backup_dir);
        let passed = exists && writable;
        let msg = if !exists {
            format!("backup directory missing: {}", backup_dir.display())
        } else if !writable {
            format!("backup directory not writable: {}", backup_dir.display())
        } else {
            format!("backup directory accessible: {}", backup_dir.display())
        };
        Self::push_check(
            checks,
            blocking,
            warnings,
            "B_backup_directory",
            passed,
            true,
            msg,
        );
    }

    fn check_logs_directory(
        &self,
        checks: &mut Vec<DeploymentReadinessCheck>,
        blocking: &mut Vec<String>,
        warnings: &mut Vec<String>,
    ) {
        let log_dir = resolve_log_dir();
        let (passed, msg) = match &log_dir {
            None => (
                false,
                "cannot resolve application log directory".to_string(),
            ),
            Some(dir) => {
                if !dir.exists() {
                    (false, format!("logs directory missing: {}", dir.display()))
                } else if !probe_dir_writable(dir) {
                    (
                        false,
                        format!("logs directory not writable: {}", dir.display()),
                    )
                } else {
                    (
                        true,
                        format!("logs directory accessible: {}", dir.display()),
                    )
                }
            }
        };
        Self::push_check(
            checks,
            blocking,
            warnings,
            "C_logs_directory",
            passed,
            true,
            msg,
        );
    }

    fn check_signing_key(
        &self,
        checks: &mut Vec<DeploymentReadinessCheck>,
        blocking: &mut Vec<String>,
        warnings: &mut Vec<String>,
    ) {
        let is_prod = std::env::var("GRPC_ENV").is_ok()
            && std::env::var("GRPC_ENV")
                .unwrap()
                .eq_ignore_ascii_case("production");

        let key_present = resolve_active_signing_key_id().is_some();
        let key_resolves =
            crate::infrastructure::security::resolve_package_signing_key_32().is_ok();
        // SEC-007 (ADR-0047): sync packages no longer consume this key; it is
        // required in production ONLY for the fiscal closure package HMAC
        // envelope ([arch:allow-hmac-fiscal] see ADR-0047).
        let passed = if is_prod {
            key_present && key_resolves
        } else {
            key_present || key_resolves
        };
        let msg = if passed {
            format!(
                "signing key available (id={})",
                resolve_active_signing_key_id().unwrap_or_else(|| "implicit".into())
            )
        } else if is_prod {
            "active signing key required in production (GRPC_PACKAGE_SIGNING_KEY / GRPC_ACTIVE_SIGNING_KEY_ID)".into()
        } else {
            "package signing key not configured".into()
        };
        Self::push_check(
            checks,
            blocking,
            warnings,
            "D_signing_key",
            passed,
            is_prod,
            msg,
        );
    }

    fn check_migration_consistency(
        &self,
        checks: &mut Vec<DeploymentReadinessCheck>,
        blocking: &mut Vec<String>,
    ) -> Result<(), AppError> {
        use crate::repositories::RepositoryProvider;
        let current = self.executor.system().get_max_schema_version()?;
        let expected = expected_schema_version();
        let passed = current == expected;
        let msg = if passed {
            format!("schema_version={} matches expected", current)
        } else {
            format!(
                "schema_version={} expected={} (migrations incomplete or drift)",
                current, expected
            )
        };
        Self::push_check(
            checks,
            blocking,
            &mut Vec::new(),
            "E_migration_consistency",
            passed,
            true,
            msg,
        );
        Ok(())
    }

    fn check_single_open_fiscal_year(
        &self,
        checks: &mut Vec<DeploymentReadinessCheck>,
        blocking: &mut Vec<String>,
    ) -> Result<(), AppError> {
        use crate::repositories::RepositoryProvider;
        let open_count = self.executor.fiscal_year_status().count_open_years()?;
        let passed = open_count == 1;
        let msg = match open_count {
            0 => "no open fiscal year".to_string(),
            1 => "exactly one open fiscal year".to_string(),
            n => format!("{} fiscal years open simultaneously", n),
        };
        Self::push_check(
            checks,
            blocking,
            &mut Vec::new(),
            "F_single_open_fiscal_year",
            passed,
            true,
            msg,
        );
        Ok(())
    }

    fn check_settings_current_year(
        &self,
        checks: &mut Vec<DeploymentReadinessCheck>,
        blocking: &mut Vec<String>,
    ) -> Result<(), AppError> {
        use crate::repositories::RepositoryProvider;
        let open_year = self.executor.fiscal_year_status().get_open_year()?;
        let current_year = self.executor.settings().get_current_year()?;

        let passed = match open_year {
            Some(y) => y == current_year,
            None => false,
        };

        let msg = if passed {
            format!("settings.current_year={} matches open year", current_year)
        } else {
            format!(
                "settings.current_year={} does not match open fiscal year {:?}",
                current_year, open_year
            )
        };
        Self::push_check(
            checks,
            blocking,
            &mut Vec::new(),
            "G_settings_current_year",
            passed,
            true,
            msg,
        );
        Ok(())
    }

    fn check_archive_invariants(
        &self,
        checks: &mut Vec<DeploymentReadinessCheck>,
        blocking: &mut Vec<String>,
        warnings: &mut Vec<String>,
    ) -> Result<(), AppError> {
        use crate::repositories::RepositoryProvider;
        let (v1, v2, v3) = self
            .executor
            .fiscal_year_status()
            .count_invariant_violations()?;
        let violations = v1 + v2 + v3;
        let passed = violations == 0;
        let msg = if passed {
            "archived fiscal years satisfy immutability invariants".to_string()
        } else {
            format!(
                "archive invariant violations: not_closed={} missing_closed_at={} open_and_archived={}",
                v1, v2, v3
            )
        };
        Self::push_check(
            checks,
            blocking,
            warnings,
            "H_archive_integrity",
            passed,
            true,
            msg,
        );
        Ok(())
    }

    fn check_restore_journal_leftovers(
        &self,
        checks: &mut Vec<DeploymentReadinessCheck>,
        blocking: &mut Vec<String>,
    ) {
        let journal = self.db_path.with_extension("restore.journal");
        let journal_tmp = journal.with_extension("restore.journal.tmp");
        let leftover = journal.exists() || journal_tmp.exists();
        let passed = !leftover;
        let msg = if passed {
            "no restore journal leftovers".to_string()
        } else {
            format!(
                "restore journal leftover detected ({} or {})",
                journal.display(),
                journal_tmp.display()
            )
        };
        Self::push_check(
            checks,
            blocking,
            &mut Vec::new(),
            "I_restore_journal",
            passed,
            true,
            msg,
        );
    }

    fn check_sqlite_integrity(
        &self,
        checks: &mut Vec<DeploymentReadinessCheck>,
        blocking: &mut Vec<String>,
    ) -> Result<(), AppError> {
        use crate::repositories::RepositoryProvider;
        let pragma_results = self.executor.system().check_integrity()?;
        let passed = !pragma_results.is_empty() && pragma_results[0] == "ok";
        let msg = if passed {
            "PRAGMA integrity_check ok".to_string()
        } else {
            format!("PRAGMA integrity_check: {:?}", pragma_results)
        };
        Self::push_check(
            checks,
            blocking,
            &mut Vec::new(),
            "J_sqlite_integrity",
            passed,
            true,
            msg,
        );
        Ok(())
    }
}

fn probe_dir_writable(dir: &Path) -> bool {
    let random_id: u64 = rand::random();
    let probe = dir.join(format!(".grpc_write_probe_{}", random_id));
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
    {
        Ok(mut f) => {
            let _ = f.write_all(b"x");
            let _ = fs::remove_file(&probe);
            true
        }
        Err(_) => {
            if probe.exists() {
                let _ = fs::remove_file(&probe);
            }
            false
        }
    }
}

impl crate::architecture::Service for DeploymentReadinessService<'_> {}
