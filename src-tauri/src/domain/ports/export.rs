use std::io;

/// Port for Excel export operations
pub trait ExcelPort: Send + Sync {
    /// Export products to Excel
    fn export_products(&self, products: &[crate::models::product::Product]) -> io::Result<Vec<u8>>;

    /// Export daily reports to Excel
    fn export_daily_reports(
        &self,
        reports: &[crate::models::report::DailyReport],
    ) -> io::Result<Vec<u8>>;

    /// Export monthly summary to Excel
    fn export_monthly_summary(
        &self,
        summaries: &[crate::models::report::MonthlySummary],
    ) -> io::Result<Vec<u8>>;

    /// Export wilaya monthly status to Excel
    fn export_wilaya_monthly_status(
        &self,
        summary: &crate::models::report::WilayaReportSummary,
    ) -> io::Result<Vec<u8>>;

    /// Export unit inventory to Excel
    fn export_unit_inventory(&self, view: &crate::models::UnitInventoryView)
        -> io::Result<Vec<u8>>;

    /// Export stock movements to Excel
    fn export_stock_movements(
        &self,
        movements: &[crate::models::inventory::StockMovement],
    ) -> io::Result<Vec<u8>>;

    /// Export audit log to Excel
    fn export_audit_log(&self, entries: &[crate::domain::audit::AuditEntry])
        -> io::Result<Vec<u8>>;
}
