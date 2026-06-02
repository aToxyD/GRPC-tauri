use chrono::NaiveDate;

#[derive(Debug, Clone)]
pub struct UnitId(String);

impl UnitId {
    pub fn new(value: String) -> Self {
        Self(value)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Scope validated by adapter/authz before usecase execution.
#[derive(Debug, Clone)]
pub enum ReportScope {
    Global,
    Unit(UnitId),
}

/// Query DTO for report listing endpoints.
///
/// When `fiscal_year` or `month` are supplied, they take priority over
/// `start_date`/`end_date` (the date range is derived from the fiscal year).
#[derive(Debug, Clone, Default)]
pub struct DailyReportFilters {
    pub start_date: Option<NaiveDate>,
    pub end_date: Option<NaiveDate>,
    pub fiscal_year: Option<i32>,
    pub month: Option<u32>,
}
