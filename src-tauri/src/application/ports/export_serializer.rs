//! Outbound ports: export datasets → serialized bytes (no IO, no crypto).

use crate::application::usecases::exports::types::{
    DailyReportExportDataset, MonthlySummaryExportDataset, ProductsExportDataset,
};
use crate::errors::AppResult;

pub trait SerializeProductsPackage: Send + Sync {
    fn products_to_package_bytes(&self, dataset: &ProductsExportDataset) -> AppResult<Vec<u8>>;
}

pub trait SerializeDailyReportPackage: Send + Sync {
    fn daily_report_to_package_bytes(
        &self,
        dataset: &DailyReportExportDataset,
    ) -> AppResult<Vec<u8>>;
}

pub trait SerializeMonthlySummaryPackage: Send + Sync {
    fn monthly_summary_to_package_bytes(
        &self,
        dataset: &MonthlySummaryExportDataset,
    ) -> AppResult<Vec<u8>>;
}
