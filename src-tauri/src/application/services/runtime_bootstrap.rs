//! Shared DB-open/bootstrap sequence (ADR-0041 §5).
//!
//! Extracted from `main.rs` so the same sequence runs on eager startup (env/dev
//! app key) and on `unlock_app_key` / `initialize_app_key` (store app key). The
//! sequence is: resolve DB path → restore recovery → open `ConnectionFactory` →
//! persisted rate limiter → deployment readiness → session recovery → fiscal
//! validation → health/telemetry → audit cleanup.

use crate::db::{ConnectionFactory, Database};
use crate::domain::rate_limiter::RateLimiter;
use crate::errors::AppError;
use crate::infrastructure::backup::recover_interrupted_restore_and_orphans;
use crate::infrastructure::rate_limiter::open_persisted_rate_limiter;
use crate::infrastructure::security::file_encryption::AgeFileEncryptionProvider;

use super::{
    AuditService, DeploymentReadinessService, DeploymentReadinessStatus, OperationalSessionService,
    SystemHealthService, TelemetryEventType, TelemetryOutcome, TelemetryService,
};

/// Artifacts of a completed runtime bootstrap.
pub struct RuntimeBootstrap {
    pub db: Database,
    pub rate_limiter: RateLimiter,
}

/// Run the full DB-open/bootstrap sequence. Fail-closed: any blocking readiness
/// or fiscal failure aborts as an `AppError` (the caller decides eager exit vs
/// a failed unlock).
pub fn bootstrap_runtime() -> crate::errors::AppResult<RuntimeBootstrap> {
    let db_path = crate::db::get_db_path()?;
    log::info!(target: "grpc::runtime", "Using database file: {}", db_path.display());

    let crypto_probe = AgeFileEncryptionProvider::new();
    recover_interrupted_restore_and_orphans(&db_path, &crypto_probe)?;

    let db = ConnectionFactory::new()?;
    let db_path = db.get_connection_path()?;
    let rate_limiter = open_persisted_rate_limiter(&db_path)?;

    match DeploymentReadinessService::new(db.executor(), db_path.clone()).verify() {
        Ok(report) => {
            for w in &report.warnings {
                log::warn!(target: "grpc::deployment", "readiness warning: {}", w);
            }
            if report.status == DeploymentReadinessStatus::NotReady {
                for f in &report.blocking_failures {
                    log::error!(target: "grpc::deployment", "readiness blocking: {}", f);
                }
                return Err(AppError::Internal(
                    "Deployment readiness: NOT READY".to_string(),
                ));
            }
            log::info!(target: "grpc::deployment", "deployment readiness: READY");
        }
        Err(e) => {
            return Err(AppError::Internal(format!(
                "Deployment readiness check failed: {e}"
            )));
        }
    }

    if let Err(e) = OperationalSessionService::new(db.executor()).recover_abandoned_sessions() {
        log::warn!(target: "grpc::session", "session recovery failed: {}", e);
    }

    if let Err(msg) = super::fiscal_validation_service::validate_fiscal_state(&db) {
        log::error!(target: "grpc::fiscal", "STARTUP ABORTED — {}", msg);
        return Err(AppError::Internal(format!(
            "Fiscal state validation failed: {msg}"
        )));
    }

    let _ = SystemHealthService::get_health_report(&db); // triggers initial integrity check + telemetry
    let _ = TelemetryService::new(db.executor()).record_event(
        TelemetryEventType::RuntimeStartup,
        TelemetryOutcome::Success,
        None,
        None,
        None,
    );

    let _ = AuditService::new(db.executor()).cleanup_old_audit_logs(None);

    log::info!(target: "grpc::runtime", "System boot verification completed successfully");
    Ok(RuntimeBootstrap { db, rate_limiter })
}
