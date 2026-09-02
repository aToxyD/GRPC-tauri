use crate::domain::audit::AuditEntry;
use crate::domain::ports::export::ExcelPort;
use crate::models::inventory::StockMovement;
use crate::models::report::{DailyReport, MonthlySummary, WilayaReportSummary};
use crate::models::UnitInventoryView;
use crate::models::*;
use rust_xlsxwriter::*;
use std::io;

// ألوان الحماية المدنية
const COLOR_HEADER_BG: u32 = 0xC0392B; // أحمر الحماية المدنية
const COLOR_HEADER_FG: u32 = 0xFFFFFF; // أبيض
const COLOR_ROW_ALT: u32 = 0xFFF5F5; // وردي فاتح

#[derive(Default)]
pub struct XlsxAdapter;

impl XlsxAdapter {
    pub fn new() -> Self {
        Self
    }

    fn header_format(&self) -> Format {
        Format::new()
            .set_background_color(Color::RGB(COLOR_HEADER_BG))
            .set_font_color(Color::RGB(COLOR_HEADER_FG))
            .set_bold()
            .set_align(FormatAlign::Center)
            .set_border(FormatBorder::Thin)
            .set_font_size(11.0)
    }

    fn data_format(&self) -> Format {
        Format::new()
            .set_border(FormatBorder::Thin)
            .set_align(FormatAlign::Right)
    }

    fn alt_row_format(&self) -> Format {
        Format::new()
            .set_background_color(Color::RGB(COLOR_ROW_ALT))
            .set_border(FormatBorder::Thin)
            .set_align(FormatAlign::Right)
    }

    fn number_format(&self) -> Format {
        Format::new()
            .set_border(FormatBorder::Thin)
            .set_align(FormatAlign::Right)
            .set_num_format("#,##0.00")
    }
}

impl ExcelPort for XlsxAdapter {
    fn export_products(&self, products: &[Product]) -> io::Result<Vec<u8>> {
        let mut workbook = Workbook::new();
        let worksheet = workbook.add_worksheet();
        worksheet.set_right_to_left(true);

        let header_fmt = self.header_format();
        let headers = ["#", "الاسم", "السعر الأساسي", "السنة"];

        for (col, h) in headers.iter().enumerate() {
            worksheet
                .write_with_format(0, col as u16, *h, &header_fmt)
                .map_err(io::Error::other)?;
        }

        let num_fmt = self.number_format();
        for (idx, p) in products.iter().enumerate() {
            let row = (idx + 1) as u32;
            let fmt = if idx % 2 == 0 {
                self.data_format()
            } else {
                self.alt_row_format()
            };

            worksheet
                .write_with_format(row, 0, (idx + 1) as u32, &fmt)
                .map_err(io::Error::other)?;
            worksheet
                .write_with_format(row, 1, &p.name, &fmt)
                .map_err(io::Error::other)?;
            worksheet
                .write_with_format(row, 2, p.base_price, &num_fmt)
                .map_err(io::Error::other)?;
            worksheet
                .write_with_format(row, 3, p.year as f64, &fmt)
                .map_err(io::Error::other)?;
        }

        workbook.save_to_buffer().map_err(io::Error::other)
    }

    fn export_daily_reports(&self, reports: &[DailyReport]) -> io::Result<Vec<u8>> {
        let mut workbook = Workbook::new();
        let worksheet = workbook.add_worksheet();
        worksheet.set_right_to_left(true);

        let header_fmt = self.header_format();
        let headers = ["التاريخ", "المستفيدون", "التكلفة اليومية", "المعدل اليومي"];

        for (col, h) in headers.iter().enumerate() {
            worksheet
                .write_with_format(0, col as u16, *h, &header_fmt)
                .map_err(io::Error::other)?;
        }

        for (idx, r) in reports.iter().enumerate() {
            let row = (idx + 1) as u32;
            let fmt = if idx % 2 == 0 {
                self.data_format()
            } else {
                self.alt_row_format()
            };

            worksheet
                .write_with_format(row, 0, r.date.to_string(), &fmt)
                .map_err(io::Error::other)?;
            worksheet
                .write_with_format(row, 1, r.total_daily_beneficiaries as f64, &fmt)
                .map_err(io::Error::other)?;
            worksheet
                .write_with_format(row, 2, r.total_daily_cost, &fmt)
                .map_err(io::Error::other)?;
            worksheet
                .write_with_format(row, 3, r.total_daily_average, &fmt)
                .map_err(io::Error::other)?;
        }

        workbook.save_to_buffer().map_err(io::Error::other)
    }

    fn export_monthly_summary(&self, summaries: &[MonthlySummary]) -> io::Result<Vec<u8>> {
        let mut workbook = Workbook::new();
        let worksheet = workbook.add_worksheet();
        worksheet.set_right_to_left(true);

        let header_fmt = self.header_format();
        let headers = [
            "السنة",
            "الشهر",
            "إجمالي المستفيدين",
            "إجمالي الاستهلاك",
            "متوسط الفطور",
            "متوسط الغداء",
            "متوسط العشاء",
            "المعدل اليومي",
        ];

        for (col, h) in headers.iter().enumerate() {
            worksheet
                .write_with_format(0, col as u16, *h, &header_fmt)
                .map_err(io::Error::other)?;
        }

        for (idx, s) in summaries.iter().enumerate() {
            let row = (idx + 1) as u32;
            let fmt = if idx % 2 == 0 {
                self.data_format()
            } else {
                self.alt_row_format()
            };

            worksheet
                .write_with_format(row, 0, s.year as f64, &fmt)
                .map_err(io::Error::other)?;
            worksheet
                .write_with_format(row, 1, s.month as f64, &fmt)
                .map_err(io::Error::other)?;
            worksheet
                .write_with_format(row, 2, s.total_beneficiaries as f64, &fmt)
                .map_err(io::Error::other)?;
            worksheet
                .write_with_format(row, 3, s.total_consumption_value, &fmt)
                .map_err(io::Error::other)?;
            worksheet
                .write_with_format(row, 4, s.breakfast_average, &fmt)
                .map_err(io::Error::other)?;
            worksheet
                .write_with_format(row, 5, s.lunch_average, &fmt)
                .map_err(io::Error::other)?;
            worksheet
                .write_with_format(row, 6, s.dinner_average, &fmt)
                .map_err(io::Error::other)?;
            worksheet
                .write_with_format(row, 7, s.daily_average, &fmt)
                .map_err(io::Error::other)?;
        }

        workbook.save_to_buffer().map_err(io::Error::other)
    }

    fn export_wilaya_monthly_status(&self, summary: &WilayaReportSummary) -> io::Result<Vec<u8>> {
        let mut workbook = Workbook::new();
        let worksheet = workbook.add_worksheet();
        worksheet.set_right_to_left(true);

        let header_fmt = self.header_format();
        let headers = [
            "اسم الوحدة",
            "كود الوحدة",
            "إجمالي المستفيدين",
            "إجمالي التكلفة",
            "المعدل اليومي",
        ];

        for (col, h) in headers.iter().enumerate() {
            worksheet
                .write_with_format(0, col as u16, *h, &header_fmt)
                .map_err(io::Error::other)?;
        }

        for (idx, r) in summary.reports.iter().enumerate() {
            let row = (idx + 1) as u32;
            let fmt = if idx % 2 == 0 {
                self.data_format()
            } else {
                self.alt_row_format()
            };

            worksheet
                .write_with_format(row, 0, &r.unit_name, &fmt)
                .map_err(io::Error::other)?;
            worksheet
                .write_with_format(row, 1, &r.unit_id, &fmt)
                .map_err(io::Error::other)?;
            worksheet
                .write_with_format(row, 2, r.total_beneficiaries as f64, &fmt)
                .map_err(io::Error::other)?;
            worksheet
                .write_with_format(row, 3, r.total_cost, &fmt)
                .map_err(io::Error::other)?;
            worksheet
                .write_with_format(row, 4, r.daily_average, &fmt)
                .map_err(io::Error::other)?;
        }

        workbook.save_to_buffer().map_err(io::Error::other)
    }

    fn export_unit_inventory(&self, view: &UnitInventoryView) -> io::Result<Vec<u8>> {
        let mut workbook = Workbook::new();
        let worksheet = workbook.add_worksheet();
        worksheet.set_right_to_left(true);

        let header_fmt = self.header_format();
        let headers = [
            "اسم المنتج",
            "الرصيد الافتتاحي",
            "الوارد",
            "المستهلك",
            "الرصيد الختامي",
        ];

        for (col, h) in headers.iter().enumerate() {
            worksheet
                .write_with_format(0, col as u16, *h, &header_fmt)
                .map_err(io::Error::other)?;
        }

        for (idx, item) in view.items.iter().enumerate() {
            let row = (idx + 1) as u32;
            let fmt = if idx % 2 == 0 {
                self.data_format()
            } else {
                self.alt_row_format()
            };

            worksheet
                .write_with_format(row, 0, &item.product_name, &fmt)
                .map_err(io::Error::other)?;
            worksheet
                .write_with_format(row, 1, item.opening_stock, &fmt)
                .map_err(io::Error::other)?;
            worksheet
                .write_with_format(row, 2, item.total_in, &fmt)
                .map_err(io::Error::other)?;
            worksheet
                .write_with_format(row, 3, item.total_out, &fmt)
                .map_err(io::Error::other)?;
            worksheet
                .write_with_format(row, 4, item.computed_closing, &fmt)
                .map_err(io::Error::other)?;
        }

        workbook.save_to_buffer().map_err(io::Error::other)
    }

    fn export_stock_movements(&self, movements: &[StockMovement]) -> io::Result<Vec<u8>> {
        let mut workbook = Workbook::new();
        let worksheet = workbook.add_worksheet();
        worksheet.set_right_to_left(true);

        let header_fmt = self.header_format();
        let headers = [
            "التاريخ",
            "المنتج",
            "النوع",
            "الكمية",
            "الرصيد السابق",
            "الرصيد الحالي",
        ];

        for (col, h) in headers.iter().enumerate() {
            worksheet
                .write_with_format(0, col as u16, *h, &header_fmt)
                .map_err(io::Error::other)?;
        }

        for (idx, m) in movements.iter().enumerate() {
            let row = (idx + 1) as u32;
            let fmt = if idx % 2 == 0 {
                self.data_format()
            } else {
                self.alt_row_format()
            };

            worksheet
                .write_with_format(row, 0, m.timestamp.to_string(), &fmt)
                .map_err(io::Error::other)?;
            worksheet
                .write_with_format(row, 1, m.product_name.as_deref().unwrap_or(""), &fmt)
                .map_err(io::Error::other)?;
            worksheet
                .write_with_format(row, 2, m.movement_type.as_str(), &fmt)
                .map_err(io::Error::other)?;
            worksheet
                .write_with_format(row, 3, m.quantity, &fmt)
                .map_err(io::Error::other)?;
            worksheet
                .write_with_format(row, 4, m.balance_before, &fmt)
                .map_err(io::Error::other)?;
            worksheet
                .write_with_format(row, 5, m.balance_after, &fmt)
                .map_err(io::Error::other)?;
        }

        workbook.save_to_buffer().map_err(io::Error::other)
    }

    fn export_audit_log(&self, entries: &[AuditEntry]) -> io::Result<Vec<u8>> {
        let mut workbook = Workbook::new();
        let worksheet = workbook.add_worksheet();
        worksheet.set_right_to_left(true);

        let header_fmt = self.header_format();
        let headers = [
            "التاريخ",
            "المستخدم",
            "العملية",
            "نوع الكيان",
            "اسم الكيان",
            "الحالة",
        ];

        for (col, h) in headers.iter().enumerate() {
            worksheet
                .write_with_format(0, col as u16, *h, &header_fmt)
                .map_err(io::Error::other)?;
        }

        for (idx, e) in entries.iter().enumerate() {
            let row = (idx + 1) as u32;
            let fmt = if idx % 2 == 0 {
                self.data_format()
            } else {
                self.alt_row_format()
            };

            worksheet
                .write_with_format(row, 0, e.timestamp.clone(), &fmt)
                .map_err(io::Error::other)?;
            worksheet
                .write_with_format(row, 1, e.username.clone(), &fmt)
                .map_err(io::Error::other)?;
            worksheet
                .write_with_format(row, 2, e.action_display.clone(), &fmt)
                .map_err(io::Error::other)?;
            worksheet
                .write_with_format(row, 3, e.entity_type_display.clone(), &fmt)
                .map_err(io::Error::other)?;
            worksheet
                .write_with_format(row, 4, e.entity_name.as_deref().unwrap_or(""), &fmt)
                .map_err(io::Error::other)?;
            worksheet
                .write_with_format(row, 5, e.status.as_str(), &fmt)
                .map_err(io::Error::other)?;
        }

        workbook.save_to_buffer().map_err(io::Error::other)
    }
}
