//! Shared DB-open/bootstrap sequence (ADR-0041 §5).
//!
//! Extracted from `main.rs` so the same sequence runs on eager startup (env/dev
//! app key) and on `unlock_app_key` / `initialize_app_key` (store app key). The
//! sequence is: resolve DB path → restore recovery → SEC-005 ledger overlay →
//! open `ConnectionFactory` → SEC-005 marker consumption (restore audit) →
//! persisted rate limiter → deployment readiness → session recovery → fiscal
//! validation → health/telemetry → audit cleanup.
//!
//! SEC-005 boot invariants (BR-05/BR-06):
//!   - the monotonic replay/registry overlay is applied BEFORE normal DB use;
//!   - pending restore markers are consumed idempotently (audit + archive);
//!   - any remediation failure fails closed with a clear error.

use crate::db::{ConnectionFactory, Database};
use crate::domain::audit::{AuditAction, AuditFilters, EntityType};
use crate::domain::ports::backup::{
    restore_archive_dir, restore_marker_history_path, BackupPort, RestoreLedgerSnapshot,
    RestoreMarker, RestoreMarkerCommit,
};
use crate::domain::rate_limiter::RateLimiter;
use crate::errors::{AppError, AppResult};
use crate::infrastructure::backup::{recover_interrupted_restore_and_orphans, SqliteBackupAdapter};
use crate::infrastructure::rate_limiter::open_persisted_rate_limiter;
use crate::infrastructure::security::file_encryption::AgeFileEncryptionProvider;
use crate::repositories::DbExecutor;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

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
    // SEC-005 BR-06: recovery reports whether an interrupted swap actually
    // completed (candidate became live). Journal presence alone is NOT proof:
    // a failed swap can leave the journal behind with the old DB restored.
    let recovery = recover_interrupted_restore_and_orphans(&db_path, &crypto_probe)?;

    // SEC-005 BR-05: apply pending ledger overlays before normal DB open.
    apply_pending_restore_ledger_overlays(&db_path, &crypto_probe)?;

    let db = ConnectionFactory::new()?;
    let db_path = db.get_connection_path()?;

    // SEC-005 BR-06: consume restore markers → emit AuditAction::RestoreBackup
    // into the restored DB → archive sidecars.
    consume_restore_markers(&db_path, db.executor(), recovery.restore_completed)?;

    let rate_limiter = open_persisted_rate_limiter(&db_path)?;

    // Node key store (SEC-008): the WILAYA signing identity for fiscal closure
    // exports — shared data dir, mirroring `commands/common::node_key_store`.
    // ADR-0062: resolution goes through the single identity data-dir resolver.
    // The pre-existing `temp_dir()` fallback still applies when the platform
    // data directory itself cannot be resolved; a misconfigured override is
    // never silently redirected.
    let node_key_store =
        crate::infrastructure::identity::NodeKeyStore::new(identity_data_dir_or_temp_fallback()?);

    match DeploymentReadinessService::new(db.executor(), db_path.clone(), &db, &node_key_store)
        .verify()
    {
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

/// Identity data directory for the node key store (ADR-0062).
///
/// The pre-existing `temp_dir().join("GRPC")` fallback is preserved for the one
/// condition it was written for — the platform data directory being
/// unresolvable. A rejected `GRPC_IDENTITY_DATA_DIR` and a tripped §5 shadow
/// guard are propagated instead, so a misconfiguration aborts boot rather than
/// redirecting identity state to an unintended directory.
fn identity_data_dir_or_temp_fallback() -> AppResult<PathBuf> {
    match crate::infrastructure::identity::data_dir::identity_data_dir() {
        Ok(dir) => Ok(dir),
        Err(AppError::Internal(_)) => {
            Ok(std::env::temp_dir().join(crate::infrastructure::identity::GRPC_DATA_DIR))
        }
        Err(e) => Err(e),
    }
}

/// SEC-005 BR-05 phase 1: apply every pending ledger sidecar as a monotonic
/// overlay on the (possibly just-restored) database. Runs after interrupted
/// restore recovery and BEFORE the normal application connection opens.
///
/// Idempotent: union/max semantics mean re-applying an already-applied
/// snapshot changes nothing. Any error aborts boot (fail closed).
pub fn apply_pending_restore_ledger_overlays(
    db_path: &Path,
    crypto: &AgeFileEncryptionProvider,
) -> AppResult<()> {
    let sidecars = discover_restore_sidecars(db_path)?;
    if sidecars.is_empty() {
        return Ok(());
    }

    let adapter = SqliteBackupAdapter::new(db_path, *crypto);
    for sidecar in &sidecars {
        let raw = std::fs::read(sidecar).map_err(|e| {
            AppError::Internal(format!(
                "قراءة أداة تراكب السجل فشلت ({}): {e}",
                sidecar.display()
            ))
        })?;
        let snapshot: RestoreLedgerSnapshot = serde_json::from_slice(&raw).map_err(|e| {
            AppError::Internal(format!(
                "أداة تراكب السجل غير صالحة ({}): {e}",
                sidecar.display()
            ))
        })?;
        adapter
            .apply_ledger_snapshot(&snapshot)
            .map_err(|e| AppError::Internal(format!("فشل تطبيق تراكب السجل بعد الاستعادة: {e}")))?;
        log::info!(
            target: "grpc::backup",
            "[RESTORE_OVERLAY_APPLIED] sidecar={}",
            sidecar.display()
        );
    }
    Ok(())
}

/// SEC-005 BR-06 phase 2: consume pending restore markers after the restored
/// database is open. Emits `AuditAction::RestoreBackup` (deduplicated by
/// marker_id) into the restored audit chain and archives each ledger sidecar.
///
/// An audit event is emitted only for a COMMITTED restore: the swap either
/// completed before restart (commit record appended by the restore command) or
/// was interrupted and completed by boot recovery (`recovery_completed_swap`).
/// Mere attempts (marker without commit, no completion) are archived WITHOUT an
/// audit event — never a false success.
///
/// Crash-safe: re-running after a crash between audit emission and archiving
/// finds the event already present and only completes the archive step.
pub fn consume_restore_markers(
    db_path: &Path,
    executor: DbExecutor<'_>,
    recovery_completed_swap: bool,
) -> AppResult<()> {
    let sidecars = discover_restore_sidecars(db_path)?;
    if sidecars.is_empty() {
        return Ok(());
    }

    let history_path = restore_marker_history_path(db_path);
    let history_raw = std::fs::read(&history_path).map_err(|e| {
        AppError::Internal(format!(
            "قراءة سجل الاستعادة الخارجي فشلت ({}): {e}",
            history_path.display()
        ))
    })?;
    let mut markers: HashMap<String, RestoreMarker> = HashMap::new();
    let mut commits: HashMap<String, RestoreMarkerCommit> = HashMap::new();
    for line in String::from_utf8_lossy(&history_raw).lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        // Parse the marker first: RestoreMarker is a superset of
        // RestoreMarkerCommit, so without this order serde would happily
        // deserialize a marker line into a commit (unknown fields ignored)
        // and every marker would be lost — a silent audit/commit loss.
        if let Ok(marker) = serde_json::from_str::<RestoreMarker>(line) {
            markers.insert(marker.marker_id.clone(), marker);
            continue;
        }
        if let Ok(commit) = serde_json::from_str::<RestoreMarkerCommit>(line) {
            commits.insert(commit.marker_id.clone(), commit);
            continue;
        }
        return Err(AppError::Internal(format!(
            "سطر سجل استعادة غير صالح: {line}"
        )));
    }

    let audit = AuditService::new(executor);
    let archive_dir = restore_archive_dir(db_path);

    for sidecar in &sidecars {
        let marker_id = marker_id_from_sidecar(db_path, sidecar).ok_or_else(|| {
            AppError::Internal(format!(
                "تعذّر استخراج marker_id من أداة الاستعادة: {}",
                sidecar.display()
            ))
        })?;
        let marker = markers.get(&marker_id).ok_or_else(|| {
            AppError::Internal(format!(
                "أداة استعادة بدون سجل علامة (marker) — يلزم تدخل يدوي: {marker_id}"
            ))
        })?;

        let committed = commits.contains_key(&marker_id) || recovery_completed_swap;
        if committed && !restore_audit_already_emitted(&audit, &marker_id)? {
            let backup_name = Path::new(&marker.backup_path)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| marker.backup_path.clone());
            let marker_value = serde_json::to_value(marker)
                .map_err(|e| AppError::Internal(format!("serialize marker: {e}")))?;
            audit
                .log_success(
                    "system",
                    "system",
                    AuditAction::RestoreBackup,
                    EntityType::Backup,
                    Some(&marker_id),
                    Some(&backup_name),
                    None,
                    Some(marker_value),
                    None,
                    None,
                )
                .map_err(|e| {
                    AppError::Internal(format!(
                        "فشل تسجيل استعادة النسخة الاحتياطية في السجل الأمني: {e}"
                    ))
                })?;
            log::info!(
                target: "grpc::backup",
                "[RESTORE_AUDIT_EMITTED] marker={} backup={}",
                marker_id,
                marker.backup_path
            );
        } else if !committed {
            log::info!(
                target: "grpc::backup",
                "[RESTORE_ATTEMPT_UNCOMMITTED] marker={} — archived without audit (no commit evidence)",
                marker_id
            );
        }

        std::fs::create_dir_all(&archive_dir)
            .map_err(|e| AppError::Internal(format!("إنشاء مجلد أرشيف الاستعادة فشل: {e}")))?;
        let archived = archive_dir.join(sidecar.file_name().ok_or_else(|| {
            AppError::Internal(format!("اسم أداة استعادة غير صالح: {}", sidecar.display()))
        })?);
        std::fs::rename(sidecar, &archived).map_err(|e| {
            AppError::Internal(format!(
                "أرشفة أداة الاستعادة فشلت ({}): {e}",
                sidecar.display()
            ))
        })?;
        log::info!(
            target: "grpc::backup",
            "[RESTORE_SIDECAR_ARCHIVED] marker={}",
            marker_id
        );
    }
    Ok(())
}

/// True when the audit chain of the restored DB already contains a
/// `RestoreBackup` event for this marker_id (idempotency check).
fn restore_audit_already_emitted(audit: &AuditService<'_>, marker_id: &str) -> AppResult<bool> {
    let filters = AuditFilters {
        action: Some(AuditAction::RestoreBackup.as_str().to_string()),
        entity_type: Some(EntityType::Backup.as_str().to_string()),
        ..Default::default()
    };
    let page = audit
        .get_audit_entries(&filters, 0, 1000)
        .map_err(|e| AppError::Internal(format!("قراءة سجل الأحداث فشلت: {e}")))?;
    Ok(page
        .entries
        .iter()
        .any(|e| e.entity_id.as_deref() == Some(marker_id)))
}

/// Discover pending ledger sidecars for this database, sorted for
/// deterministic processing order.
fn discover_restore_sidecars(db_path: &Path) -> AppResult<Vec<PathBuf>> {
    let parent = db_path
        .parent()
        .ok_or_else(|| AppError::Internal("مسار قاعدة البيانات بدون مجلد أب".to_string()))?;
    let file_name = db_path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| AppError::Internal("اسم ملف قاعدة بيانات غير صالح".to_string()))?;
    let prefix = format!("{file_name}.restore.ledger.");

    let mut out = Vec::new();
    for entry in std::fs::read_dir(parent)
        .map_err(|e| AppError::Internal(format!("قراءة مجلد قاعدة البيانات فشلت: {e}")))?
    {
        let entry = entry.map_err(|e| AppError::Internal(format!("قراءة إدخال مجلد فشل: {e}")))?;
        let path = entry.path();
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n,
            None => continue,
        };
        if name.starts_with(&prefix) && name.ends_with(".json") {
            out.push(path);
        }
    }
    out.sort();
    Ok(out)
}

/// Extract the marker_id embedded in a sidecar file name.
fn marker_id_from_sidecar(db_path: &Path, sidecar: &Path) -> Option<String> {
    let file_name = db_path.file_name().and_then(|n| n.to_str())?;
    let prefix = format!("{file_name}.restore.ledger.");
    let name = sidecar.file_name().and_then(|n| n.to_str())?;
    name.strip_prefix(&prefix)?
        .strip_suffix(".json")
        .map(String::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ports::backup::restore_ledger_sidecar_path;

    /// ADR-0062: the `temp_dir()/GRPC` fallback is retained at F for the one
    /// historical `AppError::Internal` condition only. A `Configuration`
    /// rejection must propagate, never reach the fallback.
    #[test]
    fn the_temp_fallback_is_limited_to_internal_errors() {
        use crate::infrastructure::identity::data_dir::IDENTITY_DATA_DIR_ENV;
        use crate::infrastructure::security::test_support::lock_security_test_env;

        let _env = lock_security_test_env();
        let temp_fallback =
            std::env::temp_dir().join(crate::infrastructure::identity::GRPC_DATA_DIR);

        // A rejected override is a Configuration error, not an Internal one, so
        // it must not be rewritten into the fallback directory.
        std::env::set_var(IDENTITY_DATA_DIR_ENV, "relative-identity-dir");
        let rejected = identity_data_dir_or_temp_fallback();
        std::env::remove_var(IDENTITY_DATA_DIR_ENV);

        match rejected {
            Err(e) => assert!(
                matches!(e, AppError::Configuration(_)),
                "only Internal may be converted; got {e:?}"
            ),
            Ok(dir) => assert_ne!(
                dir, temp_fallback,
                "a rejected override must never resolve to the temp fallback"
            ),
        }
    }

    #[test]
    fn marker_id_extraction_roundtrips() {
        let db_path = Path::new("/tmp/grpc/live.db");
        let sidecar = restore_ledger_sidecar_path(db_path, "abc-123");
        assert_eq!(
            marker_id_from_sidecar(db_path, &sidecar).as_deref(),
            Some("abc-123")
        );
        assert_eq!(
            sidecar.file_name().unwrap().to_str().unwrap(),
            "live.db.restore.ledger.abc-123.json"
        );
        assert_eq!(
            restore_marker_history_path(db_path),
            Path::new("/tmp/grpc/live.db.restore.history")
        );
    }
}
