//! Operational Anomaly Service
//!
//! Read-only analysis service that surfaces anomalous operational patterns
//! to the operator. The service:
//!   - NEVER repairs anything
//!   - NEVER prevents any operation
//!   - NEVER mutates business data
//!
//! It produces an immutable list of `OperationalFinding` instances which the UI
//! displays. Findings are also persisted append-only into
//! `operational_findings_log` for historical visibility.
//!
//! Anomalies covered:
//!   A — Product with outbound movements far above its 3-month average
//!   B — Abnormal jump in inventory value / stock quantity
//!   C — Daily report count below expected for a sustained period
//!   D — High number of failed imports
//!   E — Repeated failed integrity verifications

use crate::errors::AppError;
use crate::repositories::executor::DbExecutor;
use serde::{Deserialize, Serialize};

// ─── Public DTOs ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FindingSeverity {
    Info,
    Warning,
    Critical,
}

impl FindingSeverity {
    pub fn as_str(&self) -> &'static str {
        match self {
            FindingSeverity::Info => "INFO",
            FindingSeverity::Warning => "WARNING",
            FindingSeverity::Critical => "CRITICAL",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FindingCategory {
    StockMovement,
    InventoryValuation,
    ReportingActivity,
    ImportPipeline,
    IntegrityVerification,
}

impl FindingCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            FindingCategory::StockMovement => "STOCK_MOVEMENT",
            FindingCategory::InventoryValuation => "INVENTORY_VALUATION",
            FindingCategory::ReportingActivity => "REPORTING_ACTIVITY",
            FindingCategory::ImportPipeline => "IMPORT_PIPELINE",
            FindingCategory::IntegrityVerification => "INTEGRITY_VERIFICATION",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationalFinding {
    pub severity: FindingSeverity,
    pub category: FindingCategory,
    pub code: String,
    pub message: String,
    pub recommendation: String,
    /// Optional, opaque, JSON-serializable context for the UI (never used by logic).
    pub context: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationalAnalysisReport {
    pub generated_at: String,
    pub findings: Vec<OperationalFinding>,
}

// ─── Tunable thresholds (deterministic, constant — no learning, no automation) ──
//
// All thresholds are explicit constants — the system never adjusts them at runtime.

/// A product is considered to have an abnormal outbound spike when current-month
/// outbound is at least this multiplier above the 3-month average.
const OUTBOUND_SPIKE_RATIO: f64 = 3.0;

/// Minimum 3-month average outbound below which we ignore the product entirely.
/// Prevents false positives on products that simply moved from 0 to 1.
const OUTBOUND_SPIKE_MIN_AVG: f64 = 5.0;

/// Inventory value jump ratio (current vs previous snapshot of the same year).
const INVENTORY_JUMP_RATIO: f64 = 2.0;

/// Minimum prior inventory value below which a jump is ignored.
const INVENTORY_JUMP_MIN_BASE: f64 = 1000.0;

/// Window (days) used to measure "sustained low daily-report activity".
const REPORT_ACTIVITY_WINDOW_DAYS: i64 = 14;

/// Minimum expected report count in the window before raising Anomaly C.
const REPORT_ACTIVITY_MIN_EXPECTED: i64 = 5;

/// Threshold of failed imports (last 30 days) before raising Anomaly D.
const FAILED_IMPORTS_THRESHOLD: i64 = 5;

/// Threshold of failed integrity attempts (last 30 days) before Anomaly E.
const FAILED_INTEGRITY_THRESHOLD: i64 = 3;

// ─── Service ─────────────────────────────────────────────────────────────────

pub struct OperationalAnomalyService<'a> {
    executor: DbExecutor<'a>,
}

impl<'a> OperationalAnomalyService<'a> {
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }

    /// Run detectors only — no persistence (used for preflight guards).
    pub fn analyze(&self) -> Result<OperationalAnalysisReport, AppError> {
        let mut findings = Vec::new();

        self.detect_outbound_spikes(&mut findings)?;
        self.detect_inventory_value_jumps(&mut findings)?;
        self.detect_low_reporting_activity(&mut findings)?;
        self.detect_failed_imports(&mut findings)?;
        self.detect_failed_integrity_attempts(&mut findings)?;

        Ok(OperationalAnalysisReport {
            generated_at: chrono::Utc::now().to_rfc3339(),
            findings,
        })
    }

    /// Run analysis and persist findings atomically when called inside `with_transaction`.
    pub fn run_operational_analysis(&self) -> Result<OperationalAnalysisReport, AppError> {
        let report = self.analyze()?;
        self.persist_findings(&report)?;
        Ok(report)
    }

    // ── Anomaly A — Outbound spike vs 3-month average ────────────────────────
    fn detect_outbound_spikes(
        &self,
        findings: &mut Vec<OperationalFinding>,
    ) -> Result<(), AppError> {
        let now = chrono::Utc::now();
        let current_start = now - chrono::Duration::days(30);
        let baseline_start = now - chrono::Duration::days(120);
        let baseline_end = now - chrono::Duration::days(30);

        use crate::repositories::RepositoryProvider;
        let rows = self.executor.anomaly().fetch_outbound_stats(
            &current_start.to_rfc3339(),
            &baseline_start.to_rfc3339(),
            &baseline_end.to_rfc3339(),
        )?;

        for (product_id, current_qty, baseline_qty) in rows {
            // Baseline window covers 3 months → divide by 3 for monthly average.
            let monthly_avg = baseline_qty / 3.0;
            if monthly_avg < OUTBOUND_SPIKE_MIN_AVG {
                continue;
            }
            if current_qty > monthly_avg * OUTBOUND_SPIKE_RATIO {
                findings.push(OperationalFinding {
                    severity: FindingSeverity::Warning,
                    category: FindingCategory::StockMovement,
                    code: "ANOMALY_A_OUTBOUND_SPIKE".to_string(),
                    message: format!(
                        "حركة الخروج للمنتج {} هذا الشهر ({:.2}) هي {:.1} أضعاف متوسطه لـ 3 أشهر ({:.2}).",
                        product_id,
                        current_qty,
                        current_qty / monthly_avg.max(0.0001),
                        monthly_avg
                    ),
                    recommendation:
                        "يوصى بـ: التحقق من عمليات نفاد المخزون الأخيرة وتأكيد أنماط الاستهلاك مع المشغلين."
                            .to_string(),
                    context: Some(serde_json::json!({
                        "product_id": product_id,
                        "current_qty": current_qty,
                        "baseline_monthly_avg": monthly_avg,
                    })),
                });
            }
        }
        Ok(())
    }

    // ── Anomaly B — Inventory value / quantity jumps between snapshots ───────
    fn detect_inventory_value_jumps(
        &self,
        findings: &mut Vec<OperationalFinding>,
    ) -> Result<(), AppError> {
        // Compare the last two snapshots in the same fiscal year.
        use crate::repositories::RepositoryProvider;
        let pairs = self.executor.anomaly().fetch_inventory_value_jump_pairs()?;

        for p in pairs {
            if p.prev_value < INVENTORY_JUMP_MIN_BASE {
                continue;
            }
            if p.curr_value > p.prev_value * INVENTORY_JUMP_RATIO {
                findings.push(OperationalFinding {
                    severity: FindingSeverity::Warning,
                    category: FindingCategory::InventoryValuation,
                    code: "ANOMALY_B_INVENTORY_JUMP".to_string(),
                    message: format!(
                        "قفزت قيمة المخزون للسنة المالية {} من {:.2} ({}) إلى {:.2} ({}).",
                        p.fiscal_year, p.prev_value, p.prev_date, p.curr_value, p.curr_date
                    ),
                    recommendation:
                        "يوصى بـ: مراجعة حركات الدخول الأخيرة، الأرصدة الافتتاحية، وتعديلات تكلفة المنتج قبل تصدير التقارير المالية."
                            .to_string(),
                    context: Some(serde_json::json!({
                        "fiscal_year": p.fiscal_year,
                        "previous_value": p.prev_value,
                        "current_value": p.curr_value,
                        "ratio": p.curr_value / p.prev_value.max(0.0001),
                    })),
                });
            }
        }
        Ok(())
    }

    // ── Anomaly C — Sustained low daily-report activity ──────────────────────
    fn detect_low_reporting_activity(
        &self,
        findings: &mut Vec<OperationalFinding>,
    ) -> Result<(), AppError> {
        let window_start =
            (chrono::Utc::now() - chrono::Duration::days(REPORT_ACTIVITY_WINDOW_DAYS)).to_rfc3339();

        use crate::repositories::RepositoryProvider;
        let count = self
            .executor
            .anomaly()
            .count_recent_reports(&window_start)?;

        if count < REPORT_ACTIVITY_MIN_EXPECTED {
            findings.push(OperationalFinding {
                severity: FindingSeverity::Warning,
                category: FindingCategory::ReportingActivity,
                code: "ANOMALY_C_LOW_REPORTING".to_string(),
                message: format!(
                    "تم تسجيل {} تقارير يومية فقط في آخر {} يومًا (المتوقع ≥ {}).",
                    count, REPORT_ACTIVITY_WINDOW_DAYS, REPORT_ACTIVITY_MIN_EXPECTED
                ),
                recommendation:
                    "يوصى بـ: تأكيد روتين التقارير مع مشغلي الميدان؛ والتحقق مما إذا كانت الوحدات تقوم بمزامنة الحزم."
                        .to_string(),
                context: Some(serde_json::json!({
                    "window_days": REPORT_ACTIVITY_WINDOW_DAYS,
                    "report_count": count,
                    "expected_min": REPORT_ACTIVITY_MIN_EXPECTED,
                })),
            });
        }
        Ok(())
    }

    // ── Anomaly D — High number of failed imports (last 30 days) ─────────────
    fn detect_failed_imports(
        &self,
        findings: &mut Vec<OperationalFinding>,
    ) -> Result<(), AppError> {
        let window_start = (chrono::Utc::now() - chrono::Duration::days(30)).to_rfc3339();

        use crate::repositories::RepositoryProvider;
        let repos = self.executor.anomaly();
        let failed_events = repos.count_failed_import_events(&window_start)?;
        let stale_imports = repos.count_stale_import_conflicts(&window_start)?;

        let total = failed_events + stale_imports;
        if total >= FAILED_IMPORTS_THRESHOLD {
            findings.push(OperationalFinding {
                severity: FindingSeverity::Warning,
                category: FindingCategory::ImportPipeline,
                code: "ANOMALY_D_FAILED_IMPORTS".to_string(),
                message: format!(
                    "تم اكتشاف {} أحداث استيراد فاشلة/محظورة في آخر 30 يومًا (الأحداث={}، المتأخرة={}).",
                    total, failed_events, stale_imports
                ),
                recommendation:
                    "يوصى بـ: التحقق من اتساق مفتاح التوقيع بين الولاية والوحدة، ثم إعادة محاولة الاستيراد يدويًا."
                        .to_string(),
                context: Some(serde_json::json!({
                    "failed_audit_events": failed_events,
                    "stale_imports": stale_imports,
                    "threshold": FAILED_IMPORTS_THRESHOLD,
                })),
            });
        }
        Ok(())
    }

    // ── Anomaly E — Repeated failed integrity verifications ──────────────────
    fn detect_failed_integrity_attempts(
        &self,
        findings: &mut Vec<OperationalFinding>,
    ) -> Result<(), AppError> {
        let window_start = (chrono::Utc::now() - chrono::Duration::days(30)).to_rfc3339();

        use crate::repositories::RepositoryProvider;
        let failed = self
            .executor
            .anomaly()
            .count_recent_failed_integrity_attempts(&window_start)?;

        if failed >= FAILED_INTEGRITY_THRESHOLD {
            findings.push(OperationalFinding {
                severity: FindingSeverity::Critical,
                category: FindingCategory::IntegrityVerification,
                code: "ANOMALY_E_INTEGRITY_FAILURES".to_string(),
                message: format!(
                    "تم تسجيل {} عمليات تحقق من السلامة فاشلة في آخر 30 يومًا (الحد الأقصى {}).",
                    failed, FAILED_INTEGRITY_THRESHOLD
                ),
                recommendation:
                    "يوصى بـ: أخذ نسخة احتياطية جديدة، ثم تشغيل فحص كامل للسلامة أثناء فترة صيانة غير نشطة؛ وتصعيد الأمر إذا استمر الفشل."
                        .to_string(),
                context: Some(serde_json::json!({
                    "failed_attempts_30d": failed,
                    "threshold": FAILED_INTEGRITY_THRESHOLD,
                })),
            });
        }
        Ok(())
    }

    // ── Persistence (append-only) ────────────────────────────────────────────
    fn persist_findings(&self, report: &OperationalAnalysisReport) -> Result<(), AppError> {
        use crate::repositories::RepositoryProvider;
        let repo = self.executor.anomaly();
        for f in &report.findings {
            let ctx = f.context.as_ref().map(|v| v.to_string());

            repo.log_finding(
                &report.generated_at,
                f.severity.as_str(),
                f.category.as_str(),
                &f.code,
                &f.message,
                &f.recommendation,
                ctx,
            )?;
        }
        Ok(())
    }
}

impl crate::architecture::Service for OperationalAnomalyService<'_> {}
