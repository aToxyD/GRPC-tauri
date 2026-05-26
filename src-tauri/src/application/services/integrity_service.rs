use crate::errors::AppError;
use crate::repositories::executor::DbExecutor;
use crate::repositories::RepositoryProvider;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum IntegrityStatus {
    Ok,
    Warning,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum FindingSeverity {
    Warning,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrityFinding {
    pub code: String,
    pub severity: FindingSeverity,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrityReport {
    pub status: IntegrityStatus,
    pub findings: Vec<IntegrityFinding>,
}

pub struct IntegrityService<'a> {
    executor: DbExecutor<'a>,
}

impl<'a> IntegrityService<'a> {
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }

    /// Persist the integrity report to `integrity_verification_attempts` and
    /// `operational_findings_log` for audit trail and anomaly detection.
    pub fn persist(&self, report: &IntegrityReport) -> Result<(), AppError> {
        let now = chrono::Utc::now().to_rfc3339();
        let outcome = if report.findings.is_empty() {
            "PASS"
        } else {
            "FAIL"
        };

        self.executor.integrity().record_attempt(
            &now,
            "DATABASE_INTEGRITY",
            outcome,
            Some(&serde_json::to_string(report).unwrap_or_default()),
        )?;

        let anomaly_repo = self.executor.anomaly();
        for f in &report.findings {
            let severity_str = match f.severity {
                FindingSeverity::Critical => "CRITICAL",
                FindingSeverity::Warning => "WARNING",
            };
            anomaly_repo.log_finding(
                &now,
                severity_str,
                "INTEGRITY_VERIFICATION",
                &f.code,
                &f.message,
                "",
                None,
            )?;
        }
        Ok(())
    }

    pub fn run(&self) -> Result<IntegrityReport, AppError> {
        let mut findings = Vec::new();

        self.check_fifo_inventory_reconciliation(&mut findings)?;
        self.check_negative_layers(&mut findings)?;
        self.check_orphan_consumptions(&mut findings)?;
        self.check_consumption_costs(&mut findings)?;
        self.check_daily_report_totals(&mut findings)?;
        self.check_current_fiscal_year(&mut findings)?;

        let status = if findings
            .iter()
            .any(|f| f.severity == FindingSeverity::Critical)
        {
            IntegrityStatus::Critical
        } else if !findings.is_empty() {
            IntegrityStatus::Warning
        } else {
            IntegrityStatus::Ok
        };

        Ok(IntegrityReport { status, findings })
    }

    fn push(
        &self,
        findings: &mut Vec<IntegrityFinding>,
        code: &str,
        severity: FindingSeverity,
        message: String,
    ) {
        findings.push(IntegrityFinding {
            code: code.to_string(),
            severity,
            message,
        });
    }

    fn check_fifo_inventory_reconciliation(
        &self,
        findings: &mut Vec<IntegrityFinding>,
    ) -> Result<(), AppError> {
        for (unit_id, product_id, fifo_qty, stock_qty) in self
            .executor
            .integrity()
            .fetch_fifo_inventory_mismatches()?
        {
            self.push(
                findings,
                "INV_FIFO_MISMATCH",
                FindingSeverity::Critical,
                format!(
                    "unit={} product={}: fifo_qty={} inventory_qty={} diff={}",
                    unit_id,
                    product_id,
                    fifo_qty,
                    stock_qty,
                    (fifo_qty - stock_qty).abs(),
                ),
            );
        }
        Ok(())
    }

    fn check_negative_layers(&self, findings: &mut Vec<IntegrityFinding>) -> Result<(), AppError> {
        for (id, unit_id, product_id, qty) in self.executor.integrity().fetch_negative_layers()? {
            self.push(
                findings,
                "NEGATIVE_FIFO_LAYER",
                FindingSeverity::Critical,
                format!(
                    "layer={} unit={} product={}: qty_remaining={}",
                    id, unit_id, product_id, qty,
                ),
            );
        }
        Ok(())
    }

    fn check_orphan_consumptions(
        &self,
        findings: &mut Vec<IntegrityFinding>,
    ) -> Result<(), AppError> {
        for (id, movement_id) in self.executor.integrity().fetch_orphan_consumptions()? {
            self.push(
                findings,
                "ORPHAN_LAYER_CONSUMPTION",
                FindingSeverity::Critical,
                format!(
                    "consumption_record={} references non-existent movement_id={}",
                    id, movement_id,
                ),
            );
        }
        Ok(())
    }

    fn check_consumption_costs(
        &self,
        findings: &mut Vec<IntegrityFinding>,
    ) -> Result<(), AppError> {
        for (id, qty, unit_cost, total_cost) in self
            .executor
            .integrity()
            .fetch_invalid_consumption_costs()?
        {
            self.push(
                findings,
                "INVALID_CONSUMPTION_COST",
                FindingSeverity::Warning,
                format!(
                    "consumption_record={}: qty={} unit_cost={} recorded_total={} expected={}",
                    id,
                    qty,
                    unit_cost,
                    total_cost,
                    qty * unit_cost,
                ),
            );
        }
        Ok(())
    }

    fn check_daily_report_totals(
        &self,
        findings: &mut Vec<IntegrityFinding>,
    ) -> Result<(), AppError> {
        for (id, recorded, computed) in self
            .executor
            .integrity()
            .fetch_daily_report_total_mismatches()?
        {
            self.push(
                findings,
                "DAILY_REPORT_TOTAL_MISMATCH",
                FindingSeverity::Warning,
                format!(
                    "report={}: recorded_total={} computed_from_meals={} diff={}",
                    id,
                    recorded,
                    computed,
                    (recorded - computed).abs(),
                ),
            );
        }
        Ok(())
    }

    fn check_current_fiscal_year(
        &self,
        findings: &mut Vec<IntegrityFinding>,
    ) -> Result<(), AppError> {
        for (year, status) in self
            .executor
            .integrity()
            .fetch_invalid_current_fiscal_years()?
        {
            let detail = match status {
                None => format!("current_year={} has no fiscal_year_status row", year),
                Some(s) => format!("current_year={} has status '{}', expected 'open'", year, s),
            };
            self.push(
                findings,
                "INVALID_CURRENT_FISCAL_YEAR",
                FindingSeverity::Critical,
                detail,
            );
        }
        Ok(())
    }
}
