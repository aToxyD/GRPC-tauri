//! Apply decrypted daily report interchange package on Wilaya node.

use chrono::Utc;

use crate::application::services::ImportSyncService;
use crate::application::sync::{
    source_id_allowed_for_unit, validate_daily_report_package_for_import, ImportedPackageRegistry,
    SyncPackage,
};
use crate::application::usecases::exports::types::DailyReportExportDataset;
use crate::errors::{AppError, AppResult, BusinessLogicError, ValidationError};
use crate::models::{DailyConsumptionItem, DailyReport, DailyReportResult};
use crate::repositories::{DbExecutor, UnitRepository};

pub const DAILY_REPORT_PACKAGE_KIND: &str = "daily_report";

#[derive(Debug, Clone)]
pub struct ImportDailyReportPackageInput {
    pub package: SyncPackage<DailyReportExportDataset>,
    pub unit_id: String,
    pub importer_wilaya_code: String,
    pub imported_by: String,
}

#[derive(Debug, Clone)]
pub struct ImportDailyReportPackageOutcome {
    pub report_count: usize,
    pub item_count: usize,
    pub unit_id: String,
    pub package_id: String,
}

pub fn execute(
    executor: DbExecutor<'_>,
    registry: &impl ImportedPackageRegistry,
    input: ImportDailyReportPackageInput,
) -> AppResult<ImportDailyReportPackageOutcome> {
    validate_daily_report_package_for_import(&input.package)?;

    let unit_trim = input.unit_id.trim();
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
                resource: "تقرير_يومي_sync".into(),
                from_wilaya: unit_row.wilaya_code.clone(),
                to_wilaya: importer_wilaya_trim.to_string(),
            },
        ));
    }
    if !source_id_allowed_for_unit(
        input.package.metadata.source_node_id.as_str(),
        &unit_row,
        importer_wilaya_trim,
    ) {
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

    let snapshot = input.package.payload.snapshot;
    use chrono::Datelike;
    let report = DailyReport {
        id: snapshot.report_id,
        date: snapshot.date,
        personnel_count: snapshot.personnel_count,
        guest_count: snapshot.guest_count,
        total_meals_cost: snapshot.total_meals_cost,
        actual_meal_rate: snapshot.actual_meal_rate,
        unit_id: Some(unit_trim.to_string()),
        created_at: Utc::now(),
        fiscal_year: snapshot.date.year(),
    };
    let report_id = report.id.clone();
    let items: Vec<DailyConsumptionItem> = snapshot
        .items
        .into_iter()
        .map(|i| DailyConsumptionItem {
            id: uuid::Uuid::new_v4().to_string(),
            daily_report_id: report_id.clone(),
            product_id: i.product_id,
            product_name: i.product_name,
            quantity: i.quantity,
            unit_price: i.unit_price,
            total_cost: i.total_cost,
        })
        .collect();

    let item_count = items.len();
    let imported = ImportSyncService::new(executor)
        .import_daily_reports(vec![DailyReportResult { report, items }])?;
    registry.mark_imported(&package_id)?;

    Ok(ImportDailyReportPackageOutcome {
        report_count: imported,
        item_count,
        unit_id: unit_trim.to_string(),
        package_id: package_id.0.clone(),
    })
}
