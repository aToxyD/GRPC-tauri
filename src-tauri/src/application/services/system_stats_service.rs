use crate::application::services::SettingsService;
use crate::db::Database;
use crate::domain::rate_limiter::RateLimiter;
use crate::errors::AppError;
use crate::models::{LoginMetrics, SystemMetrics};
use crate::repositories::RepositoryProvider;
use chrono::{Datelike, Utc};

pub struct SystemStatsService;

impl SystemStatsService {
    pub fn new(_executor: crate::repositories::DbExecutor<'_>) -> Self {
        Self
    }

    /// Compute metrics that were previously built inside `commands/system.rs`.
    /// All filesystem/database coordination happens here.
    pub fn get_system_metrics(
        db: &Database,
        process_start_time: std::time::Instant,
    ) -> Result<SystemMetrics, AppError> {
        let executor = db.executor();
        let settings_svc = SettingsService::new(executor);

        let effective_unit_id = {
            let settings = settings_svc.get_settings()?;
            if settings.node_type == crate::models::NodeType::Unit {
                settings_svc.get_current_unit_id()?
            } else {
                None
            }
        };

        let db_path = db.get_connection_path()?;
        let db_size = match std::fs::metadata(&db_path) {
            Ok(m) => m.len(),
            Err(e) => {
                log::warn!("Failed to read DB file metadata: {:?}", e);
                0
            }
        };

        let backup_count = Self::compute_backup_count();
        let last_backup = Self::compute_last_backup();

        let total_products = executor.products().list_products()?.len() as u32;

        let report_repo = executor.reports();
        let daily_reports = match effective_unit_id.as_deref() {
            Some(unit_id) => report_repo
                .list_daily_reports_scoped(None, None, unit_id)?
                .len() as u32,
            None => report_repo.list_daily_reports(None, None)?.len() as u32,
        };

        let now = Utc::now();
        let current_month = now.month();
        let current_year = now.year();

        let monthly_reports = Self::count_monthly_reports(
            db,
            effective_unit_id.as_deref(),
            current_month,
            current_year,
        )?;

        let today_orders = Self::count_today_orders(db);

        let uptime = process_start_time.elapsed().as_secs();

        let memory_usage = (db_size / 1024 / 1024) as u64;

        let units_count = Self::compute_units_count(db)?;

        let active_users = executor.users().count_active_users()? as u32;

        Ok(SystemMetrics {
            database_size: db_size,
            backup_count,
            uptime,
            memory_usage,
            total_products,
            units_count,

            last_backup,
            daily_reports,
            monthly_reports,
            today_orders,
            active_users,
        })
    }

    pub fn get_login_metrics(rate_limiter: &RateLimiter) -> LoginMetrics {
        let failed_attempts = rate_limiter.get_total_failed_attempts();
        let total_attempts = rate_limiter.get_total_attempts();
        let locked_users = rate_limiter.get_locked_users_count();
        let any_locked = locked_users > 0;

        let mut lockout_time = None;
        if any_locked {
            // Keep the exact key priority/behavior as before.
            let keys = ["admin", "user", "admin1", "user1"];
            for key in keys {
                if let Some(remaining) = rate_limiter.get_remaining_lockout_secs(key) {
                    lockout_time = Some(remaining);
                    break;
                }
            }
        }

        let lockout_reason = if any_locked {
            Some("تجاوز عدد المحاولات المسموح بها".to_string())
        } else {
            None
        };

        LoginMetrics {
            total_attempts,
            failed_attempts,
            current_lockout: any_locked,
            lockout_time_remaining: lockout_time,
            lockout_reason,
        }
    }

    fn compute_backup_count() -> u32 {
        let backup_dir = match dirs::data_dir() {
            Some(d) => d.join("GRPC").join("backups"),
            None => std::path::PathBuf::from("./backups"),
        };

        if !backup_dir.exists() || !backup_dir.is_dir() {
            return 0;
        }

        match std::fs::read_dir(&backup_dir) {
            Ok(entries) => entries.count() as u32,
            Err(e) => {
                log::warn!("Failed to read backups directory: {:?}", e);
                0
            }
        }
    }

    fn compute_last_backup() -> Option<String> {
        let backup_dir = match dirs::data_dir() {
            Some(d) => d.join("GRPC").join("backups"),
            None => std::path::PathBuf::from("./backups"),
        };

        if !backup_dir.exists() || !backup_dir.is_dir() {
            return None;
        }

        match std::fs::read_dir(&backup_dir) {
            Ok(entries) => {
                let mut latest_time: Option<std::time::SystemTime> = None;
                for entry in entries.flatten() {
                    let metadata = match entry.metadata() {
                        Ok(m) => m,
                        Err(e) => {
                            log::warn!("Failed to read backup entry metadata: {:?}", e);
                            continue;
                        }
                    };

                    let modified = match metadata.modified() {
                        Ok(t) => t,
                        Err(e) => {
                            log::warn!("Failed to read backup modified time: {:?}", e);
                            continue;
                        }
                    };

                    let keep_new = match latest_time {
                        Some(t) => modified > t,
                        None => true,
                    };
                    if keep_new {
                        latest_time = Some(modified);
                    }
                }

                let latest = latest_time?;

                // Convert SystemTime -> RFC3339
                let since_epoch = match latest.duration_since(std::time::UNIX_EPOCH) {
                    Ok(d) => d,
                    Err(e) => {
                        log::warn!("Backup time is before UNIX_EPOCH: {:?}", e);
                        return None;
                    }
                };
                let secs = since_epoch.as_secs() as i64;

                chrono::DateTime::<chrono::Utc>::from_timestamp(secs, 0).map(|dt| dt.to_rfc3339())
            }
            Err(e) => {
                log::warn!("Failed to read backups directory entries: {:?}", e);
                None
            }
        }
    }

    fn count_monthly_reports(
        db: &Database,
        effective_unit_id: Option<&str>,
        current_month: u32,
        current_year: i32,
    ) -> Result<u32, AppError> {
        db.executor().reports().count_daily_reports_by_month(
            current_year,
            current_month,
            effective_unit_id,
        )
    }

    fn count_today_orders(db: &Database) -> u32 {
        let today = Utc::now().date_naive().to_string();
        match db.executor().orders().count_today_orders(&today) {
            Ok(count) => count,
            Err(e) => {
                log::warn!("Failed to count today orders: {:?}", e);
                0
            }
        }
    }

    fn compute_units_count(db: &Database) -> Result<u32, AppError> {
        let executor = db.executor();
        let settings = match SettingsService::new(executor).get_settings() {
            Ok(s) => s,
            Err(e) => {
                log::warn!("Failed to load settings for units_count: {:?}", e);
                return Ok(0);
            }
        };

        let wilaya_code = match settings.wilaya_code.as_deref() {
            Some(code) => code,
            None => return Ok(0),
        };

        match executor.units().list_units(wilaya_code) {
            Ok(units) => Ok(units.len() as u32),
            Err(e) => {
                log::warn!("Failed to list units for wilaya_code: {:?}", e);
                Ok(0)
            }
        }
    }
}
