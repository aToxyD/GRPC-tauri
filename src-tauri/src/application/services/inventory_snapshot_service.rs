//! Inventory Snapshot Service Module
//!
//! Handles computing and storing monthly inventory snapshots, staleness, and views.

use crate::errors::AppError;
use crate::models::ComputeSnapshotResult;
use crate::repositories::{DbExecutor, RepositoryProvider};

/// Service for handling inventory snapshots
pub struct InventorySnapshotService<'a> {
    executor: DbExecutor<'a>,
}

impl<'a> InventorySnapshotService<'a> {
    /// Create a new InventorySnapshotService
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }

    /// حساب وتخزين لقطة مخزون شهرية للوحدة
    pub fn compute_and_store_unit_snapshot(
        &self,
        unit_id: &str,
        year: i32,
        month: u32,
        force_recompute: bool,
    ) -> Result<ComputeSnapshotResult, AppError> {
        let inv_repo = self.executor.inventory();
        let unit_repo = self.executor.units();
        let product_repo = self.executor.products();

        // 1. جلب معلومات الوحدة
        let unit = unit_repo
            .get_unit(unit_id)?
            .ok_or_else(|| AppError::Internal(format!("الوحدة {} غير موجودة", unit_id)))?;
        let unit_name = unit.name;

        // 2. فحص الـ Cache (بدون force)
        if !force_recompute {
            let existing_count = inv_repo.check_existing_snapshot_count(unit_id, year, month)?;

            if existing_count > 0 {
                // تحقق من الـ freshness قبل إرجاع الـ cache
                let month_start = format!("{year}-{:02}-01T00:00:00Z", month);
                let month_end = format!(
                    "{year}-{:02}-{:02}T23:59:59Z",
                    month,
                    days_in_month(year, month)
                );
                inv_repo.refresh_staleness(unit_id, year, month, &month_start, &month_end)?;

                let balance_anom =
                    inv_repo.count_snapshot_balance_anomalies(unit_id, year, month)?;
                let consumption_anom =
                    inv_repo.count_snapshot_consumption_anomalies(unit_id, year, month)?;

                return Ok(ComputeSnapshotResult {
                    unit_id: unit_id.to_string(),
                    unit_name,
                    year,
                    month,
                    products_computed: existing_count as usize,
                    balance_anomalies: balance_anom as usize,
                    consumption_anomalies: consumption_anom as usize,
                    already_existed: true,
                });
            }
        }

        // 3. حساب نطاق الشهر
        let month_start = format!("{}-{:02}-01T00:00:00Z", year, month);
        let month_end = format!(
            "{}-{:02}-{:02}T23:59:59Z",
            year,
            month,
            days_in_month(year, month)
        );

        // 4. جلب المنتجات التي تحركت خلال الشهر لهذه الوحدة فقط
        let products = inv_repo.get_active_products_for_month(unit_id, &month_start, &month_end)?;

        if products.is_empty() {
            return Ok(ComputeSnapshotResult {
                unit_id: unit_id.to_string(),
                unit_name,
                year,
                month,
                products_computed: 0,
                balance_anomalies: 0,
                consumption_anomalies: 0,
                already_existed: false,
            });
        }

        // 5. حذف القديم إذا force
        if force_recompute {
            inv_repo.delete_snapshot(unit_id, year, month)?;
        }

        let now = chrono::Utc::now().to_rfc3339();
        let mut balance_anomalies = 0usize;
        let mut consumption_anomalies = 0usize;

        // 6. حساب وحفظ snapshots لكل منتج
        for (product_id, product_name) in &products {
            // التحقق من وجود المنتج
            if product_repo.get_product(product_id)?.is_none() {
                continue;
            }

            let opening_stock = inv_repo.get_opening_stock(product_id, unit_id, &month_start)?;
            let total_in = inv_repo.get_total_in(product_id, unit_id, &month_start, &month_end)?;
            let total_out =
                inv_repo.get_total_out(product_id, unit_id, &month_start, &month_end)?;

            // المعادلة: Closing = Opening + IN - OUT
            let computed_closing = (opening_stock + total_in - total_out).max(0.0);

            // Reported closing = balance_after of last consumption movement
            let reported_closing = inv_repo.get_reported_closing(
                product_id,
                unit_id,
                &month_start,
                &month_end,
                computed_closing,
            )?;

            let variance = reported_closing - computed_closing;

            // منتج جديد في أول شهر
            let is_new_product_first_month =
                opening_stock.abs() < f64::EPSILON && total_in > f64::EPSILON;

            let has_balance_anomaly = if is_new_product_first_month {
                false
            } else {
                variance.abs() > 0.01
            };

            // Consumption Anomaly: متوسط 3 أشهر سابقة
            let three_months_ago = subtract_months(year, month, 3);
            let prev_start = format!(
                "{}-{:02}-01T00:00:00Z",
                three_months_ago.0, three_months_ago.1
            );
            let avg_consumption_3months = inv_repo.get_avg_consumption_3months(
                product_id,
                unit_id,
                &prev_start,
                &month_start,
            )?;

            let has_consumption_anomaly = match avg_consumption_3months {
                Some(avg) if avg > 0.0 => total_out > 1.5 * avg,
                _ => false,
            };

            let is_stale = false;

            if has_balance_anomaly {
                balance_anomalies += 1;
            }
            if has_consumption_anomaly {
                consumption_anomalies += 1;
            }

            // Insert snapshot
            let snapshot_id = uuid::Uuid::new_v4().to_string();
            inv_repo.insert_snapshot(
                &snapshot_id,
                unit_id,
                &unit_name,
                year,
                month,
                product_id,
                product_name,
                opening_stock,
                total_in,
                total_out,
                computed_closing,
                reported_closing,
                variance,
                has_balance_anomaly,
                avg_consumption_3months,
                has_consumption_anomaly,
                is_stale,
                &now,
            )?;
        }

        Ok(ComputeSnapshotResult {
            unit_id: unit_id.to_string(),
            unit_name,
            year,
            month,
            products_computed: products.len(),
            balance_anomalies,
            consumption_anomalies,
            already_existed: false,
        })
    }

    /// تحديث حالة staleness للقطة (live recheck)
    pub fn refresh_staleness(&self, unit_id: &str, year: i32, month: u32) -> Result<(), AppError> {
        let repo = self.executor.inventory();
        let month_start = format!("{year}-{:02}-01T00:00:00Z", month);
        let month_end = format!(
            "{year}-{:02}-{:02}T23:59:59Z",
            month,
            days_in_month(year, month)
        );
        repo.refresh_staleness(unit_id, year, month, &month_start, &month_end)
    }

    /// Get the inventory view for a unit
    pub fn get_unit_inventory_view(
        &self,
        unit_id: &str,
        year: i32,
        month: u32,
    ) -> Result<Option<crate::models::UnitInventoryView>, AppError> {
        self.refresh_staleness(unit_id, year, month)?;

        let repo = self.executor.inventory();
        let count = repo.check_existing_snapshot_count(unit_id, year, month)?;
        if count == 0 {
            return Ok(None);
        }

        let items = repo.get_unit_monthly_snapshots(unit_id, year, month)?;

        let balance_anomaly_count = items.iter().filter(|i| i.has_balance_anomaly).count();
        let consumption_anomaly_count = items.iter().filter(|i| i.has_consumption_anomaly).count();
        let stale_count = items.iter().filter(|i| i.is_stale).count();

        Ok(Some(crate::models::UnitInventoryView {
            unit_id: unit_id.to_string(),
            unit_name: items
                .first()
                .map(|i| i.unit_name.clone())
                .unwrap_or_default(),
            report_year: year,
            report_month: month,
            has_any_anomaly: balance_anomaly_count + consumption_anomaly_count > 0,
            balance_anomaly_count,
            consumption_anomaly_count,
            stale_count,
            computed_at: items
                .first()
                .map(|i| i.computed_at.clone())
                .unwrap_or_default(),
            items,
            total_products: count as usize,
        }))
    }

    /// جلب الأشهر المتاحة للتقارير لوحدة معينة
    pub fn get_available_report_months_for_unit(
        &self,
        unit_id: &str,
    ) -> Result<Vec<(i32, u32)>, AppError> {
        let repo = self.executor.inventory();
        repo.get_available_report_months_for_unit(unit_id)
    }
}

// Helper: أيام الشهر
pub fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) {
                29
            } else {
                28
            }
        }
        _ => 30,
    }
}

// Helper: طرح أشهر
fn subtract_months(year: i32, month: u32, n: u32) -> (i32, u32) {
    let total = (year as i64 * 12) + (month as i64 - 1) - n as i64;
    let new_year = (total / 12) as i32;
    let new_month = (total % 12 + 1) as u32;
    (new_year, new_month)
}
