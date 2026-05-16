use crate::errors::AppError;
use crate::repositories::executor::DbExecutor;
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
        let orphan:i64=self.executor.query_row("SELECT COUNT(DISTINCT fiscal_year) FROM opening_balance_snapshots WHERE fiscal_year NOT IN (SELECT year FROM fiscal_year_status)",[],|r|r.get(0))?;
        if orphan > 0 {
            warnings.push(FiscalIntegrityWarning {
                code: "DRIFT_A".into(),
                details: format!("{} سنوات لقطات بدون سنة مالية", orphan),
            })
        }
        let invalid_movements:i64=self.executor.query_row(r#"SELECT COUNT(*) FROM stock_movements sm JOIN fiscal_year_status fys ON fys.year=sm.fiscal_year WHERE fys.status='closed' AND fys.closed_at IS NOT NULL AND sm.timestamp>fys.closed_at"#,[],|r|r.get(0))?;
        if invalid_movements > 0 {
            warnings.push(FiscalIntegrityWarning {
                code: "DRIFT_B".into(),
                details: format!("{} حركات مخزون بعد إغلاق السنة", invalid_movements),
            })
        }
        let bad_reports:i64=self.executor.query_row(r#"SELECT COUNT(*) FROM daily_reports dr CROSS JOIN settings s WHERE dr.fiscal_year!=s.current_year"#,[],|r|r.get(0))?;
        if bad_reports > 0 {
            warnings.push(FiscalIntegrityWarning {
                code: "DRIFT_C".into(),
                details: format!("{} تقارير بسنة مالية غير متطابقة", bad_reports),
            })
        }
        let open_count: i64 = self.executor.query_row(
            "SELECT COUNT(*) FROM fiscal_year_status WHERE status='open'",
            [],
            |r| r.get(0),
        )?;
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
