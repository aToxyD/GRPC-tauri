//! Import Sync Service

use crate::errors::AppError;
use crate::models::{DailyReportMeal, DailyReportResult, ProductSyncRecord};
use crate::repositories::{DbExecutor, RepositoryProvider};
use chrono::{Datelike, Utc};

pub struct ImportSyncService<'a> {
    executor: DbExecutor<'a>,
}

impl<'a> ImportSyncService<'a> {
    fn validate_import_fiscal_year(
        &self,
        incoming_year: i32,
        package_type: &str,
        source_node: &str,
        package_id: &str,
    ) -> Result<(), AppError> {
        let current_year: i32 =
            self.executor
                .query_row("SELECT current_year FROM settings WHERE id=1", [], |r| {
                    r.get(0)
                })?;
        crate::application::services::FiscalHistoricalGuard::new(self.executor)
            .assert_import_year_allowed(incoming_year)?;
        crate::application::services::FiscalYearService::new(self.executor)
            .assert_fiscal_year_open(incoming_year)?;
        if package_type != "historical_import" && incoming_year != current_year {
            log::warn!(target:"grpc::sync","[FISCAL_IMPORT_REJECTED] source_node={} package_id={} incoming_year={} current_year={}",source_node,package_id,incoming_year,current_year);
            return Err(AppError::Internal(
                "Fiscal year mismatch during import".into(),
            ));
        }
        Ok(())
    }

    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }

    pub fn get_current_node_id(db: &crate::db::Database) -> String {
        match crate::application::services::SettingsService::new(db.executor()).get_settings() {
            Ok(s) => s.unit_name.unwrap_or_else(|| "WILAYA".to_string()),
            Err(_) => "unknown".to_string(),
        }
    }

    pub fn is_incoming_newer(incoming: &str, existing: &str) -> bool {
        match (
            chrono::DateTime::parse_from_rfc3339(incoming),
            chrono::DateTime::parse_from_rfc3339(existing),
        ) {
            (Ok(incoming_dt), Ok(existing_dt)) => {
                incoming_dt.with_timezone(&chrono::Utc) > existing_dt.with_timezone(&chrono::Utc)
            }
            _ => incoming > existing,
        }
    }

    pub fn import_daily_reports(
        &self,
        reports: Vec<DailyReportResult>,
    ) -> Result<usize, AppError> {
        let mut count = 0;
        let now = Utc::now().to_rfc3339();

        let report_repo = self.executor.reports();
        let product_repo = self.executor.products();

        for report_result in reports {
            let fiscal_year = report_result.report.fiscal_year;

            self.validate_import_fiscal_year(
                fiscal_year,
                "sync_import",
                "remote_node",
                &report_result.report.id,
            )?;

            let unit_id = report_result.report.unit_id.as_deref();
            let date_str = report_result.report.date.to_string();

            let exists = match unit_id {
                Some(uid) => report_repo.daily_report_exists_for_date_unit(&date_str, uid)?,
                None => report_repo.daily_report_exists_for_date_global(&date_str)?,
            };

            if !exists {
                let report_id = uuid::Uuid::new_v4().to_string();
                report_repo.insert_raw_daily_report(&report_id, &report_result.report, &now)?;

                for meal_result in &report_result.meals {
                    let meal_id = uuid::Uuid::new_v4().to_string();
                    let meal = DailyReportMeal {
                        id: meal_id.clone(),
                        daily_report_id: report_id.clone(),
                        ..meal_result.meal
                    };
                    report_repo.insert_raw_meal(&meal)?;

                    for item in &meal_result.items {
                        if product_repo.product_exists(&item.product_id)? {
                            report_repo.insert_meal_item(
                                &uuid::Uuid::new_v4().to_string(),
                                &meal_id,
                                &item.product_id,
                                item.quantity,
                                item.unit_price,
                                item.total_cost,
                            )?;
                        }
                    }
                }

                count += 1;
            }
        }

        Ok(count)
    }

    pub fn import_monthly_report(
        &self,
        unit_id: &str,
        _year: i32,
        _month: u32,
        reports: Vec<DailyReportResult>,
    ) -> Result<usize, AppError> {
        let count = reports.len();
        let report_repo = self.executor.reports();

        for report in reports {
            let report_id = uuid::Uuid::new_v4().to_string();
            let now = Utc::now().to_rfc3339();
            let fiscal_year = report.report.fiscal_year;

            self.validate_import_fiscal_year(
                fiscal_year,
                "sync_import",
                "remote_node",
                &report.report.id,
            )?;

            report_repo.insert_or_replace_raw_daily_report(&report_id, &report.report, &now)?;

            for meal_result in report.meals {
                let meal_id = uuid::Uuid::new_v4().to_string();
                let meal = DailyReportMeal {
                    id: meal_id.clone(),
                    daily_report_id: report_id.clone(),
                    ..meal_result.meal
                };
                report_repo.insert_raw_meal(&meal)?;

                for item in meal_result.items {
                    report_repo.insert_raw_meal_item(&item)?;
                }
            }
        }
        Ok(count)
    }

    pub fn import_products_sync(
        &self,
        records: &[ProductSyncRecord],
        _current_year: i32,
        _current_node_id: &str,
    ) -> Result<(usize, usize, usize), AppError> {
        let mut imported_count = 0usize;
        let mut updated_count = 0usize;
        let mut skipped_count = 0usize;

        let product_repo = self.executor.products();
        let inventory_repo = self.executor.inventory();

        for record in records {
            let existing_ts = product_repo.get_product_updated_at(&record.id)?;

            let should_insert = match &existing_ts {
                None => true,
                Some(existing) => Self::is_incoming_newer(&record.updated_at, existing),
            };

            if !should_insert {
                skipped_count += 1;
                continue;
            }

            product_repo.upsert_product_sync(record)?;

            if existing_ts.is_some() {
                updated_count += 1;
            } else {
                imported_count += 1;
            }

            let stock_exists = inventory_repo.stock_exists_for_product(&record.id)?;

            if !stock_exists && record.deleted == 0 {
                let stock_id = uuid::Uuid::new_v4().to_string();
                inventory_repo.insert_empty_stock(
                    &stock_id,
                    &record.id,
                    &record.updated_at,
                    &record.node_id,
                )?;
            }
        }

        Ok((imported_count, updated_count, skipped_count))
    }

    pub fn import_stock_movements(
        &self,
        movements: Vec<crate::models::inventory::StockMovement>,
        unit_id: Option<&str>,
    ) -> Result<usize, AppError> {
        let mut count = 0;
        let movement_repo = self.executor.stock_movements();

        for mut m in movements {
            if let Some(fiscal_year) = m.fiscal_year {
                crate::application::services::FiscalYearService::new(self.executor)
                    .assert_fiscal_year_open(fiscal_year)?;
            } else {
                let dt = crate::errors::parse_datetime_rfc3339(&m.timestamp)?;
                let fiscal_year = dt.year();
                log::warn!(
                    target: "grpc::fiscal",
                    "FISCAL FALLBACK: stock movement timestamp-inferred fiscal_year={}",
                    fiscal_year
                );
                crate::application::services::FiscalYearService::new(self.executor)
                    .assert_fiscal_year_open(fiscal_year)?;
                m.fiscal_year = Some(fiscal_year);
            }

            if let Some(uid) = unit_id {
                m.unit_id = Some(uid.to_string());
            }

            if !movement_repo.movement_exists(&m.id)? {
                movement_repo.insert_raw_stock_movement(&m)?;
                count += 1;
            }
        }

        Ok(count)
    }
}
