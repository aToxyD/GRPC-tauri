use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QueryPlanStep {
    pub id: u64,
    pub parent: u64,
    pub detail: String,
    pub is_scan: bool,
    pub table: Option<String>,
    pub index_used: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QueryPlan {
    pub sql_hash: u64,
    pub steps: Vec<QueryPlanStep>,
    pub has_full_table_scan: bool,
    pub scan_count: usize,
    pub severity: ScanSeverity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ScanSeverity {
    None,
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TableScanWarning {
    pub step_id: u64,
    pub table: String,
    pub detail: String,
    pub severity: ScanSeverity,
    pub recommendation: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IndexRecommendation {
    pub table: String,
    pub columns: Vec<String>,
    pub reason: IndexReason,
    pub estimated_impact: ScanSeverity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IndexReason {
    FullTableScan,
    MissingFilterIndex,
    SortingWithoutIndex,
    JoinWithoutIndex,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QueryDiagnostics {
    pub plan: QueryPlan,
    pub warnings: Vec<TableScanWarning>,
    pub recommendations: Vec<IndexRecommendation>,
}

impl QueryPlanStep {
    pub fn new(id: u64, parent: u64, detail: String) -> Self {
        let lower = detail.to_lowercase();
        let is_scan = lower.contains("scan")
            && !lower.contains("using index")
            && !lower.contains("using covering index");
        let table = extract_table_name(&detail);
        let index_used = extract_index_name(&detail);
        Self {
            id,
            parent,
            detail,
            is_scan,
            table,
            index_used,
        }
    }
}

fn extract_table_name(detail: &str) -> Option<String> {
    let lower = detail.to_lowercase();
    for keyword in &["table ", "from "] {
        if let Some(pos) = lower.find(keyword) {
            let start = pos + keyword.len();
            let rest = &detail[start..];
            let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
            let name = rest[..end].trim().to_string();
            if !name.is_empty() {
                return Some(name);
            }
        }
    }
    None
}

fn extract_index_name(detail: &str) -> Option<String> {
    let lower = detail.to_lowercase();
    if let Some(pos) = lower.find("using index ") {
        let start = pos + "using index ".len();
        let rest = &detail[start..];
        let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        let name = rest[..end].trim().to_string();
        if !name.is_empty() {
            return Some(name);
        }
    }
    if let Some(pos) = lower.find("using covering index ") {
        let start = pos + "using covering index ".len();
        let rest = &detail[start..];
        let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        let name = rest[..end].trim().to_string();
        if !name.is_empty() {
            return Some(name);
        }
    }
    None
}

fn hash_sql(sql: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    sql.hash(&mut hasher);
    hasher.finish()
}

impl QueryPlan {
    pub fn from_explain_output(sql: &str, rows: Vec<(u64, u64, u64, String)>) -> Self {
        let steps: Vec<QueryPlanStep> = rows
            .into_iter()
            .map(|(id, parent, _, detail)| QueryPlanStep::new(id, parent, detail))
            .collect();

        let scans: Vec<&QueryPlanStep> = steps.iter().filter(|s| s.is_scan).collect();
        let has_full_table_scan = !scans.is_empty();
        let scan_count = scans.len();
        let severity = classify_scan_severity(scan_count, &steps);

        Self {
            sql_hash: hash_sql(sql),
            steps,
            has_full_table_scan,
            scan_count,
            severity,
        }
    }

    pub fn warnings(&self) -> Vec<TableScanWarning> {
        self.steps
            .iter()
            .filter(|s| s.is_scan)
            .map(|s| {
                let table = s.table.clone().unwrap_or_else(|| "unknown".into());
                let severity = scan_severity_for_step(s);
                let recommendation = format!(
                    "Consider adding an index on table '{}' for the query pattern",
                    table
                );
                TableScanWarning {
                    step_id: s.id,
                    table,
                    detail: s.detail.clone(),
                    severity,
                    recommendation,
                }
            })
            .collect()
    }

    pub fn recommendations(&self) -> Vec<IndexRecommendation> {
        self.steps
            .iter()
            .filter(|s| s.is_scan)
            .filter_map(|s| {
                let table = s.table.clone()?;
                Some(IndexRecommendation {
                    table,
                    columns: vec![],
                    reason: IndexReason::FullTableScan,
                    estimated_impact: scan_severity_for_step(s),
                })
            })
            .collect()
    }
}

fn classify_scan_severity(scan_count: usize, steps: &[QueryPlanStep]) -> ScanSeverity {
    if scan_count == 0 {
        return ScanSeverity::None;
    }
    let total_steps = steps.len();
    let ratio = scan_count as f64 / total_steps.max(1) as f64;
    if ratio > 0.75 {
        ScanSeverity::Critical
    } else if ratio > 0.5 {
        ScanSeverity::High
    } else if ratio > 0.25 {
        ScanSeverity::Medium
    } else {
        ScanSeverity::Low
    }
}

fn scan_severity_for_step(step: &QueryPlanStep) -> ScanSeverity {
    let lower = step.detail.to_lowercase();
    if lower.contains("scan") && (lower.contains("every") || lower.contains("entire")) {
        ScanSeverity::High
    } else if lower.contains("scan") {
        ScanSeverity::Medium
    } else {
        ScanSeverity::Low
    }
}

impl QueryDiagnostics {
    pub fn new(sql: &str, rows: Vec<(u64, u64, u64, String)>) -> Self {
        let plan = QueryPlan::from_explain_output(sql, rows);
        let warnings = plan.warnings();
        let recommendations = plan.recommendations();
        Self {
            plan,
            warnings,
            recommendations,
        }
    }

    pub fn has_warnings(&self) -> bool {
        !self.warnings.is_empty()
    }

    pub fn severity(&self) -> ScanSeverity {
        self.plan.severity
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_explain_rows() -> Vec<(u64, u64, u64, String)> {
        vec![
            (3, 0, 0, "SCAN TABLE products".into()),
            (5, 0, 0, "SEARCH TABLE inventory_stocks USING INDEX idx_stock_product (product_id=?)".into()),
        ]
    }

    fn sample_explain_no_scan() -> Vec<(u64, u64, u64, String)> {
        vec![
            (2, 0, 0, "SEARCH TABLE products USING INDEX idx_products_id (id=?)".into()),
            (4, 0, 0, "SEARCH TABLE inventory_stocks USING COVERING INDEX idx_stock_product (product_id=?)".into()),
        ]
    }

    #[test]
    fn parses_explain_output_deterministically() {
        let rows = sample_explain_rows();
        let plan1 = QueryPlan::from_explain_output("SELECT * FROM products", rows.clone());
        let plan2 = QueryPlan::from_explain_output("SELECT * FROM products", rows);
        assert_eq!(plan1, plan2);
    }

    #[test]
    fn detects_full_table_scan() {
        let rows = sample_explain_rows();
        let plan = QueryPlan::from_explain_output("SELECT * FROM products", rows);
        assert!(plan.has_full_table_scan);
        assert_eq!(plan.scan_count, 1);
    }

    #[test]
    fn no_false_positive_scan() {
        let rows = sample_explain_no_scan();
        let plan = QueryPlan::from_explain_output("SELECT * FROM products WHERE id = ?", rows);
        assert!(!plan.has_full_table_scan);
        assert_eq!(plan.severity, ScanSeverity::None);
    }

    #[test]
    fn generates_scan_warnings() {
        let rows = sample_explain_rows();
        let plan = QueryPlan::from_explain_output("SELECT * FROM products", rows);
        let warnings = plan.warnings();
        assert!(!warnings.is_empty());
        assert_eq!(warnings[0].table, "products");
    }

    #[test]
    fn generates_index_recommendations() {
        let rows = sample_explain_rows();
        let plan = QueryPlan::from_explain_output("SELECT * FROM products", rows);
        let recs = plan.recommendations();
        assert!(!recs.is_empty());
        assert_eq!(recs[0].reason, IndexReason::FullTableScan);
    }

    #[test]
    fn same_input_same_query_plan() {
        let sql = "SELECT * FROM products WHERE id = ?";
        let rows1 = sample_explain_no_scan();
        let rows2 = sample_explain_no_scan();
        let diag1 = QueryDiagnostics::new(sql, rows1);
        let diag2 = QueryDiagnostics::new(sql, rows2);
        assert_eq!(diag1, diag2);
    }

    #[test]
    fn serde_round_trip() {
        let rows = sample_explain_rows();
        let plan = QueryPlan::from_explain_output("SELECT * FROM products", rows);
        let json = serde_json::to_string(&plan).unwrap();
        let deserialized: QueryPlan = serde_json::from_str(&json).unwrap();
        assert_eq!(plan, deserialized);
    }

    #[test]
    fn query_diagnostics_has_warnings() {
        let rows = sample_explain_rows();
        let diag = QueryDiagnostics::new("SELECT * FROM products", rows);
        assert!(diag.has_warnings());
        assert_eq!(diag.severity(), ScanSeverity::Medium);
    }
}
