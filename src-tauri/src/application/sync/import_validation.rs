//! Import-side semantic validation for interchange packages.

use chrono::Datelike;

use crate::application::sync::SyncPackage;
use crate::application::usecases::exports::types::{
    ContractCatalogExportDataset, DailyReportExportDataset, MonthlySummaryExportDataset,
    ProductsExportDataset,
};
use crate::domain::validation;
use crate::errors::{AppError, AppResult, ValidationError};

/// Wire + semantic checks before any DB mutation.
pub fn validate_monthly_summary_package_for_import(
    package: &SyncPackage<MonthlySummaryExportDataset>,
) -> AppResult<()> {
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
    if package.payload.snapshot.meals.is_empty() {
        return Err(AppError::Validation(ValidationError::InvalidFormat {
            field: "meals".into(),
            message: "الحزمة لا تحتوي وجبات صالحة".into(),
        }));
    }
    Ok(())
}

/// Wire + semantic checks for a ContractCatalog package (ADR-0055 /
/// SEC-087-F). The catalog is a WILAYA-authoritative projection; a package
/// without contracts carries no projection and is rejected fail-closed
/// (mirrors the products validator's non-empty rule).
pub fn validate_contract_catalog_package_for_import(
    package: &SyncPackage<ContractCatalogExportDataset>,
) -> AppResult<()> {
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
    if package.payload.contracts.is_empty() {
        return Err(AppError::Validation(ValidationError::InvalidFormat {
            field: "contracts".into(),
            message: "الحزمة لا تحتوي عقودًا — كتالوج العقود فارغ".into(),
        }));
    }
    for row in &package.payload.contracts {
        if row.contract.id.trim().is_empty() {
            return Err(AppError::Validation(ValidationError::InvalidFormat {
                field: "contracts[].id".into(),
                message: "عقد بدون معرف".into(),
            }));
        }
        if row.contract.contract_reference.trim().is_empty() {
            return Err(AppError::Validation(ValidationError::InvalidFormat {
                field: "contracts[].contract_reference".into(),
                message: format!("عقد «{}» بدون مرجع", row.contract.id),
            }));
        }
        if row.contract.unit_id.trim().is_empty() || row.contract.supplier_id.trim().is_empty() {
            return Err(AppError::Validation(ValidationError::InvalidFormat {
                field: "contracts[].ownership".into(),
                message: format!("عقد «{}» بدون وحدة أو مورد", row.contract.id),
            }));
        }
        if row.product_lines.is_empty() {
            return Err(AppError::Validation(ValidationError::InvalidFormat {
                field: "contracts[].product_lines".into(),
                message: format!("عقد «{}» بدون منتجات", row.contract.id),
            }));
        }
    }
    Ok(())
}
