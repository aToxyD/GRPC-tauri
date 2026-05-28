//! Fiscal-year validation and startup-state checking service.
//!
//! Validates fiscal invariants without mutating state.
//! Used both at runtime (write guards) and at startup (validate_fiscal_state).

use crate::errors::AppError;
use crate::repositories::{DbExecutor, RepositoryProvider};

pub struct FiscalValidationService<'a> {
    executor: DbExecutor<'a>,
}

impl<'a> FiscalValidationService<'a> {
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }

    /// Centralized write guard — used by StockMovementService, DailyReportService, etc.
    pub fn assert_fiscal_year_open(&self, year: i32) -> Result<(), AppError> {
        self.executor.fiscal_year_status().assert_open(year)
    }
}

impl crate::architecture::Service for FiscalValidationService<'_> {}

/// Startup fiscal-state validation.
///
/// Called once from `main()` after migrations.  Stops the application with a
/// clear error if the fiscal state is inconsistent so that operators see the
/// problem immediately rather than discovering corrupt data later.
///
/// Checks:
///   A) exactly one fiscal year is 'open'
///   B) settings.current_year matches the open fiscal year
///   C) no orphan opening_balance_snapshots (next_year has snapshots but no
///      open status row)
pub fn validate_fiscal_state(db: &crate::db::Database) -> Result<(), String> {
    let executor = db.executor();

    // ── A: exactly one open year ──────────────────────────────────────────
    let open_count: i64 = executor
        .fiscal_year_status()
        .count_open_years()
        .map_err(|e| format!("fiscal_state A: cannot count open years: {}", e))?;

    if open_count == 0 {
        return Err(
            "FISCAL STATE INVALID: no fiscal year is open.              Use the fiscal management screen to open a year before starting."
                .to_string(),
        );
    }
    if open_count > 1 {
        return Err(format!(
            "FISCAL STATE INVALID: {} fiscal years are open simultaneously.              Only one may be open at a time. Investigate fiscal_year_status table.",
            open_count
        ));
    }

    // ── B: settings.current_year matches open year ────────────────────────
    let open_year: i32 = executor
        .fiscal_year_status()
        .get_open_year()
        .map_err(|e| format!("fiscal_state B: cannot read open year: {}", e))?
        .ok_or_else(|| "FISCAL STATE INVALID: open year not found after count check".to_string())?;

    let current_year: i32 = executor
        .settings()
        .get_current_year()
        .map_err(|e| format!("fiscal_state B: cannot read settings.current_year: {}", e))?;

    if current_year != open_year {
        log::error!(
            target: "grpc::fiscal",
            "[FISCAL_STATE_INVALID] settings.current_year={} open_year={}",
            current_year, open_year
        );

        return Err(format!(
            "FISCAL STATE INVALID: settings.current_year={} does not match open fiscal year={}",
            current_year, open_year
        ));
    }

    // ── C: no orphan opening_balance_snapshots ────────────────────────────
    let orphan_count: i64 = executor
        .integrity()
        .count_orphan_opening_balances()
        .map_err(|e| format!("fiscal_state C: orphan check failed: {}", e))?;

    if orphan_count > 0 {
        log::warn!(
            target: "grpc::fiscal",
            "FISCAL STATE WARN: {} orphan fiscal_year(s) found in opening_balance_snapshots              with no matching fiscal_year_status row.              This is non-critical but should be investigated.",
            orphan_count
        );
    }

    log::info!(
        target: "grpc::fiscal",
        "[FISCAL_STARTUP_OK] open_year={} settings_year={} orphan_snapshots={}",
        open_year, current_year, orphan_count
    );

    Ok(())
}
