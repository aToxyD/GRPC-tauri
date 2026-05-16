use crate::application::services::fiscal_integrity_service::FiscalIntegrityService;
use crate::errors::AppError;
use crate::repositories::executor::DbExecutor;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SystemIntegrityState {
    Healthy,
    Warning,
    Corrupted,
    Critical,
}

impl SystemIntegrityState {
    pub fn blocks_financial_operations(&self) -> bool {
        matches!(self, Self::Corrupted | Self::Critical)
    }

    /// Deterministic classification from current DB signals (same inputs → same state).
    pub fn resolve_from_executor(executor: DbExecutor<'_>) -> Result<Self, AppError> {
        use crate::repositories::RepositoryProvider;

        let pragma_results = executor.system().check_integrity()?;
        if pragma_results.is_empty() || pragma_results[0] != "ok" {
            return Ok(Self::Critical);
        }

        let audit_chain_fails = executor.integrity().count_failed_attempts("AUDIT_CHAIN")?;
        if audit_chain_fails > 0 {
            return Ok(Self::Critical);
        }

        let window_30d = (chrono::Utc::now() - chrono::Duration::days(30)).to_rfc3339();
        let failed_integrity_30d = executor
            .anomaly()
            .count_recent_failed_integrity_attempts(&window_30d)?;

        if failed_integrity_30d >= 3 {
            return Ok(Self::Critical);
        }

        let fiscal = FiscalIntegrityService::new(executor).run_full_integrity_scan()?;
        if !fiscal.ok {
            let has_severe = fiscal.warnings.iter().any(|w| {
                w.code == "DRIFT_D" || w.code == "DRIFT_E" || w.code.starts_with("DRIFT_")
            });
            if has_severe {
                return Ok(Self::Corrupted);
            }
            return Ok(Self::Warning);
        }

        if failed_integrity_30d > 0 {
            return Ok(Self::Warning);
        }

        Ok(Self::Healthy)
    }

    /// Stable error category string for startup / recovery classification tests.
    pub fn error_category(&self) -> &'static str {
        match self {
            Self::Healthy => "INTEGRITY_HEALTHY",
            Self::Warning => "INTEGRITY_WARNING",
            Self::Corrupted => "INTEGRITY_CORRUPTED",
            Self::Critical => "INTEGRITY_CRITICAL",
        }
    }
}
