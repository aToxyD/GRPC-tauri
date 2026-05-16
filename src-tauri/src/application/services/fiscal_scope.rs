use crate::errors::AppError;
use crate::repositories::{DbExecutor, RepositoryProvider};

/// Resolves the active fiscal year from persisted operational settings.
///
/// Official financial reports MUST use this helper instead of system time.
pub fn resolve_active_fiscal_year(executor: DbExecutor<'_>) -> Result<i32, AppError> {
    Ok(executor.settings().get_settings_row()?.current_year)
}
