//! Export usecase types — naming: `*Request` / `*Input` (adapter), `*Dataset` (read model out).

use serde::{Deserialize, Serialize};

use crate::models::{
    DailyDetailSyncSnapshot, DailyReportSyncSnapshot, MonthlySummary, ProductExportRow,
};

// --- Products ----------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct ExportProductsRequest;

#[derive(Debug, Clone, Default)]
pub struct ExportProductsInput;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductsExportDataset {
    pub product_rows: Vec<ProductExportRow>,
}

// --- Daily report -----------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyReportExportDataset {
    pub snapshot: DailyReportSyncSnapshot,
}

#[derive(Debug, Clone)]
pub struct DailyReportExportInput {
    pub report_id: String,
}

// --- Monthly summary --------------------------------------------------------

#[derive(Clone, Copy, Debug)]
pub struct MonthlySummaryExportInput {
    pub year: i32,
    pub month: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonthlySummaryExportDataset {
    pub summary: MonthlySummary,
    pub daily_detail_rows: Vec<DailyDetailSyncSnapshot>,
}

// --- Stock Movements --------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StockMovementsExportDataset {
    pub movements: Vec<crate::models::StockMovement>,
}

#[derive(Debug, Clone)]
pub struct StockMovementsExportInput {
    pub start_date: String,
    pub end_date: String,
}
