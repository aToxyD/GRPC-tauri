//! Import-side semantic validation for interchange packages.

use chrono::Datelike;

use crate::application::sync::{
    CompatibilityPolicy, SupportedSchemaWindow, SyncPackage, SYNC_PACKAGE_SCHEMA_VERSION,
};
use crate::application::usecases::exports::types::{
    DailyReportExportDataset, MonthlySummaryExportDataset, ProductsExportDataset,
};
use crate::domain::validation;
use crate::errors::{AppError, AppResult, ValidationError};

/// Wire + semantic checks before any DB mutation.
pub fn validate_monthly_summary_package_for_import(
    package: &SyncPackage<MonthlySummaryExportDataset>,
) -> AppResult<()> {
    let sv = package.metadata.schema_version;
    if let Err(e) = SupportedSchemaWindow::can_import(sv) {
        return Err(AppError::Validation(ValidationError::InvalidFormat {
            field: "schema_version".into(),
            message: format!(
                "[{}] إصدار المخطط {} غير مدعوم بالاستيراد (المدعوم حاليًا {})",
                e.code(),
                sv,
                SYNC_PACKAGE_SCHEMA_VERSION
            ),
        }));
    }

    if package.metadata.package_id.0.trim().is_empty() {
        return Err(AppError::Validation(ValidationError::InvalidFormat {
            field: "package_id".into(),
            message: "معرّف الحزمة مفقود".into(),
        }));
    }

    if package.metadata.source_node_id.trim().is_empty() {
        return Err(AppError::Validation(ValidationError::InvalidFormat {
            field: "source_node_id".into(),
            message: "مصدر الحزمة (العقدة) مفقود — لا يمكن الاستيراد بدون بيانات المنشأ".into(),
        }));
    }

    let y = package.payload.summary.year;
    let m = package.payload.summary.month;
    validation::validate_calendar_month(y, m)?;

    for row in &package.payload.daily_detail_rows {
        if row.date.year() != y || row.date.month() as i32 != m {
            return Err(AppError::Validation(ValidationError::InvalidFormat {
                field: "daily_detail_rows".into(),
                message: format!("صف يومي '{}' لا يطابق شهر الملخص {}/{}", row.date, m, y),
            }));
        }
    }

    Ok(())
}

pub fn validate_products_package_for_import(
    package: &SyncPackage<ProductsExportDataset>,
) -> AppResult<()> {
    let sv = package.metadata.schema_version;
    if let Err(e) = SupportedSchemaWindow::can_import(sv) {
        return Err(AppError::Validation(ValidationError::InvalidFormat {
            field: "schema_version".into(),
            message: format!(
                "[{}] إصدار المخطط {} غير مدعوم بالاستيراد (المدعوم حاليًا {})",
                e.code(),
                sv,
                SYNC_PACKAGE_SCHEMA_VERSION
            ),
        }));
    }

    if package.metadata.package_id.0.trim().is_empty() {
        return Err(AppError::Validation(ValidationError::InvalidFormat {
            field: "package_id".into(),
            message: "معرّف الحزمة مفقود".into(),
        }));
    }

    if package.metadata.source_node_id.trim().is_empty() {
        return Err(AppError::Validation(ValidationError::InvalidFormat {
            field: "source_node_id".into(),
            message: "مصدر الحزمة (العقدة) مفقود — لا يمكن الاستيراد بدون بيانات المنشأ".into(),
        }));
    }

    if package.payload.product_rows.is_empty() {
        return Err(AppError::Validation(ValidationError::InvalidFormat {
            field: "product_rows".into(),
            message: "الحزمة لا تحتوي على منتجات".into(),
        }));
    }

    Ok(())
}

pub fn validate_daily_report_package_for_import(
    package: &SyncPackage<DailyReportExportDataset>,
) -> AppResult<()> {
    let sv = package.metadata.schema_version;
    if let Err(e) = SupportedSchemaWindow::can_import(sv) {
        return Err(AppError::Validation(ValidationError::InvalidFormat {
            field: "schema_version".into(),
            message: format!(
                "[{}] إصدار المخطط {} غير مدعوم بالاستيراد (المدعوم حاليًا {})",
                e.code(),
                sv,
                SYNC_PACKAGE_SCHEMA_VERSION
            ),
        }));
    }
    if package.metadata.package_id.0.trim().is_empty() {
        return Err(AppError::Validation(ValidationError::InvalidFormat {
            field: "package_id".into(),
            message: "معرّف الحزمة مفقود".into(),
        }));
    }
    if package.metadata.source_node_id.trim().is_empty() {
        return Err(AppError::Validation(ValidationError::InvalidFormat {
            field: "source_node_id".into(),
            message: "مصدر الحزمة (العقدة) مفقود — لا يمكن الاستيراد بدون بيانات المنشأ".into(),
        }));
    }
    if package.payload.snapshot.report_id.trim().is_empty() {
        return Err(AppError::Validation(ValidationError::InvalidFormat {
            field: "report_id".into(),
            message: "الحزمة لا تحتوي معرف تقرير صالح".into(),
        }));
    }
    Ok(())
}
