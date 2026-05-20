//! Apply a decrypted monthly summary interchange package — no file I/O here.

use chrono::Utc;

use crate::application::services::MonthlyReportService;
use crate::application::sync::{
    monthly_summary_source_allowed_for_unit, validate_monthly_summary_package_for_import,
    ImportedPackageRegistry, SyncPackage,
};
use crate::application::usecases::exports::types::MonthlySummaryExportDataset;
use crate::errors::{AppError, AppResult, BusinessLogicError, ValidationError};
use crate::models::MonthlyReport;
use crate::repositories::{DbExecutor, UnitRepository};
use uuid::Uuid;

pub const MONTHLY_SUMMARY_PACKAGE_KIND: &str = "monthly_summary";

#[derive(Debug, Clone)]
pub struct ImportMonthlySummaryPackageInput {
    pub package: SyncPackage<MonthlySummaryExportDataset>,
    pub unit_id: String,
    pub importer_wilaya_code: String,
    pub imported_by: String,
}

#[derive(Debug, Clone)]
pub struct ImportMonthlySummaryPackageOutcome {
    pub report_count: i32,
    pub unit_id: String,
    pub package_id: String,
}

/// Build persistence model for Wilaya **`monthly_reports`** row (`ON CONFLICT` last-import wins).
pub fn monthly_report_from_package(
    package: &SyncPackage<MonthlySummaryExportDataset>,
    unit_id: &str,
    imported_by: &str,
) -> MonthlyReport {
    let s = &package.payload.summary;
    MonthlyReport {
        id: Uuid::new_v4().to_string(),
        unit_id: unit_id.to_string(),
        report_year: s.year,
        report_month: s.month,
        total_beneficiaries: s.total_beneficiaries,
        total_consumption_value: s.total_consumption_value,
        breakfast_average: s.breakfast_average,
        lunch_average: s.lunch_average,
        dinner_average: s.dinner_average,
        daily_average: s.daily_average,
        report_count: s.report_count,
        imported_at: Utc::now(),
        imported_by: imported_by.to_string(),
        file_hash: Some(package.metadata.package_id.0.clone()),
    }
}

pub fn execute(
    executor: DbExecutor<'_>,
    registry: &impl ImportedPackageRegistry,
    input: ImportMonthlySummaryPackageInput,
) -> AppResult<ImportMonthlySummaryPackageOutcome> {
    validate_monthly_summary_package_for_import(&input.package)?;

    let unit_trim = input.unit_id.trim();
    let imported_by_trim = input.imported_by.trim();
    let importer_wilaya_trim = input.importer_wilaya_code.trim();

    if unit_trim.is_empty() {
        return Err(AppError::Validation(ValidationError::Required {
            field: "unit_id".into(),
        }));
    }
    if importer_wilaya_trim.is_empty() {
        return Err(AppError::Validation(ValidationError::Required {
            field: "wilaya_code".into(),
        }));
    }

    let unit_row = UnitRepository::new(executor)
        .get_unit(unit_trim)?
        .ok_or_else(|| {
            AppError::BusinessLogic(BusinessLogicError::ResourceNotFound {
                resource: "الوحدة".into(),
                id: unit_trim.to_string(),
            })
        })?;

    if unit_row.wilaya_code != importer_wilaya_trim {
        return Err(AppError::BusinessLogic(
            BusinessLogicError::CrossWilayaForbidden {
                resource: "تقرير_شهري_sync".into(),
                from_wilaya: unit_row.wilaya_code.clone(),
                to_wilaya: importer_wilaya_trim.to_string(),
            },
        ));
    }

    if !monthly_summary_source_allowed_for_unit(&input.package, &unit_row, importer_wilaya_trim) {
        return Err(AppError::Validation(ValidationError::InvalidFormat {
            field: "source_node_id".into(),
            message: format!(
                "مصدر الحزمة «{}» لا يطابق الوحدة المختارة ولا ولاية المستورد",
                input.package.metadata.source_node_id.trim()
            ),
        }));
    }

    let package_id = input.package.metadata.package_id.clone();
    if registry.has_imported(&package_id)? {
        return Err(AppError::BusinessLogic(
            BusinessLogicError::DuplicateSyncPackage {
                package_id: package_id.0.clone(),
            },
        ));
    }

    let report = monthly_report_from_package(&input.package, unit_trim, imported_by_trim);
    let report_count = input.package.payload.summary.report_count;
    MonthlyReportService::new(executor).import_monthly_report(&report)?;
    registry.mark_imported(&package_id)?;

    Ok(ImportMonthlySummaryPackageOutcome {
        report_count,
        unit_id: unit_trim.to_string(),
        package_id: package_id.0.clone(),
    })
}
