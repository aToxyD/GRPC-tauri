use crate::errors::AppError;
use crate::repositories::executor::DbExecutor;
use crate::repositories::RepositoryProvider;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct FiscalIntegrityWarning {
    pub code: String,
    pub details: String,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct FiscalIntegrityReport {
    pub ok: bool,
    pub warnings: Vec<FiscalIntegrityWarning>,
}

pub struct FiscalIntegrityService<'a> {
    executor: DbExecutor<'a>,
}
impl<'a> FiscalIntegrityService<'a> {
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }
    pub fn run_full_integrity_scan(&self) -> Result<FiscalIntegrityReport, AppError> {
        let mut warnings = Vec::new();
        let orphan = self.executor.integrity().count_orphan_opening_balances()?;
        if orphan > 0 {
            warnings.push(FiscalIntegrityWarning {
                code: "DRIFT_A".into(),
                details: format!("{} سنوات لقطات بدون سنة مالية", orphan),
            })
        }
        let invalid_movements = self.executor.integrity().count_post_closure_movements()?;
        if invalid_movements > 0 {
            warnings.push(FiscalIntegrityWarning {
                code: "DRIFT_B".into(),
                details: format!("{} حركات مخزون بعد إغلاق السنة", invalid_movements),
            })
        }
        let bad_reports = self.executor.reports().count_with_fiscal_year_mismatch()?;
        if bad_reports > 0 {
            warnings.push(FiscalIntegrityWarning {
                code: "DRIFT_C".into(),
                details: format!("{} تقارير بسنة مالية غير متطابقة", bad_reports),
            })
        }
        let open_count = self.executor.fiscal_year_status().count_open_years()?;
        if open_count > 1 {
            warnings.push(FiscalIntegrityWarning {
                code: "DRIFT_D".into(),
                details: "تم اكتشاف عدة سنوات مالية مفتوحة".into(),
            })
        }
        if open_count == 0 {
            warnings.push(FiscalIntegrityWarning {
                code: "DRIFT_E".into(),
                details: "لم يتم اكتشاف أي سنة مالية مفتوحة".into(),
            })
        }
        Ok(FiscalIntegrityReport {
            ok: warnings.is_empty(),
            warnings,
        })
    }
}
