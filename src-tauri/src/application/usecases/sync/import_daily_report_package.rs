//! Apply decrypted daily report package on Wilaya node (one report + meals[]).

use chrono::Utc;

use crate::application::services::SyncImportExecutionService;
use crate::application::sync::{
    source_id_allowed_for_unit, validate_daily_report_package_for_import, ImportedPackageRegistry,
    SyncPackage,
};
use crate::application::usecases::exports::types::DailyReportExportDataset;
use crate::errors::{AppError, AppResult, BusinessLogicError, ValidationError};
use crate::models::{
    DailyConsumptionSummary, DailyReport, DailyReportMeal, DailyReportMealItem, DailyReportResult,
    MealSectionResult,
};
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
        id: snapshot.report_id.clone(),
        date: snapshot.date,
        unit_id: Some(unit_trim.to_string()),
        total_daily_cost: snapshot.total_daily_cost,
        total_daily_average: snapshot.total_daily_average,
        total_daily_beneficiaries: snapshot.total_daily_beneficiaries,
        created_at: Utc::now(),
        fiscal_year: snapshot.date.year(),
    };

    let mut meal_results = Vec::new();
    for meal_snap in snapshot.meals {
        let meal_id = uuid::Uuid::new_v4().to_string();
        let meal = DailyReportMeal {
            id: meal_id.clone(),
            daily_report_id: report.id.clone(),
            meal_type: meal_snap.meal_type,
            staff_24h_count: meal_snap.staff_24h_count,
            staff_8h_count: meal_snap.staff_8h_count,
            reservation_count: meal_snap.reservation_count,
            mission_count: meal_snap.mission_count,
            guest_count: meal_snap.guest_count,
            total_beneficiaries: meal_snap.total_beneficiaries,
            total_meal_cost: meal_snap.total_meal_cost,
            meal_average: meal_snap.meal_average,
        };
        let items: Vec<DailyReportMealItem> = meal_snap
            .items
            .into_iter()
            .map(|i| DailyReportMealItem {
                id: uuid::Uuid::new_v4().to_string(),
                meal_id: meal_id.clone(),
                product_id: i.product_id.clone(),
                product_name: i.product_name.clone(),
                quantity: i.quantity,
                unit_price: i.unit_price,
                total_cost: i.total_cost,
                fifo_layer_id: None,
            })
            .collect();
        meal_results.push(MealSectionResult { meal, items });
    }

    let item_count: usize = meal_results.iter().map(|m| m.items.len()).sum();
    let report_result = DailyReportResult {
        report,
        meals: meal_results,
        daily_summary: DailyConsumptionSummary::default(),
    };
    let imported =
        SyncImportExecutionService::new(executor).import_daily_reports(vec![report_result])?;
    registry.mark_imported(&package_id)?;

    Ok(ImportDailyReportPackageOutcome {
        report_count: imported,
        item_count,
        unit_id: unit_trim.to_string(),
        package_id: package_id.0.clone(),
    })
}
