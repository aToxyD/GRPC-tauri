use crate::errors::AppError;
use crate::repositories::system::SystemRepository;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct DatabaseGrowthReport {
    pub total_size_mb: f64,
    pub table_sizes: Vec<(String, i64)>,
    pub estimated_growth_rate: String,
    pub largest_tables: Vec<String>,
    pub recommended_actions: Vec<String>,
}

pub struct DatabaseRetentionService;
impl DatabaseRetentionService {
    pub fn analyze_database_growth(
        repo: &SystemRepository,
    ) -> Result<DatabaseGrowthReport, AppError> {
        let (page_count, page_size) = repo.get_page_stats()?;
        Ok(DatabaseGrowthReport {
            total_size_mb: (page_count * page_size) as f64 / 1024f64 / 1024f64,
            table_sizes: vec![("audit_log".into(), 0), ("stock_movements".into(), 0)],
            estimated_growth_rate: "manual-analysis-required".into(),
            largest_tables: vec!["audit_log".into(), "stock_movements".into()],
            recommended_actions: vec![
                "archive closed fiscal years".into(),
                "manual vacuum during maintenance window".into(),
            ],
        })
    }

    pub fn verify_vacuum_safety(
        has_open_tx: bool,
        restore_active: bool,
        import_active: bool,
    ) -> Result<(), AppError> {
        if has_open_tx || restore_active || import_active {
            return Err(AppError::Internal(
                "VACUUM refused during active operation".into(),
            ));
        }
        Ok(())
    }
}
