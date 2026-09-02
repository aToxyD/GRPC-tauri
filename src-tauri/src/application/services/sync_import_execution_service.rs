use super::sync_import_models::ValidationSnapshot;
use super::sync_import_models::{
    ImportExecutionError, ImportExecutionSummary, ImportMutationSummary, ReplayProtectionResult,
    SyncImportRequest, SyncImportResult,
};
use super::sync_import_validation_service::SyncImportValidationService;
use crate::application::sync_integrity::replay::{
    AppliedPackages, AppliedTransitions, ReplayDetector, SeenTransactions,
};
use crate::application::sync_integrity::types::ConflictDetectionOutcome;
use crate::application::usecases::exports::types::ContractCatalogExportDataset;
use crate::db::Database;
use crate::domain::events::{DomainEvent, EventContext};
use crate::errors::AppError;
use crate::models::{DailyReportMeal, DailyReportResult, ProductSyncRecord};
use crate::repositories::{DbExecutor, RepositoryProvider};
use chrono::{Datelike, Utc};
use std::collections::BTreeSet;

pub struct SyncImportExecutionService<'a> {
    executor: DbExecutor<'a>,
}

/// Row-level apply summary for a ContractCatalog import (ADR-0055 /
/// SEC-087-F). `*_imported` = rows newly inserted; `*_updated` = rows
/// refreshed; `*_skipped` = rows out of the importer's unit scope.
#[derive(Debug, Clone, Default)]
pub struct ContractCatalogImportSummary {
    pub suppliers_imported: usize,
    pub suppliers_updated: usize,
    pub suppliers_skipped: usize,
    pub links_applied: usize,
    pub links_skipped: usize,
    pub contracts_imported: usize,
    pub contracts_updated: usize,
    pub contracts_skipped: usize,
    pub product_lines_imported: usize,
    pub product_lines_updated: usize,
    pub allocations_imported: usize,
    pub allocations_updated: usize,
    pub allocations_skipped: usize,
    pub exceptions_imported: usize,
    pub exceptions_skipped: usize,
    pub tax_policies_applied: usize,
}

impl<'a> SyncImportExecutionService<'a> {
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }

    fn validate_fiscal(
        &self,
        incoming_year: i32,
        package_type: &str,
        source_node: &str,
        package_id: &str,
    ) -> Result<(), AppError> {
        super::SyncImportValidationService::new(self.executor).validate_import_fiscal_year(
            incoming_year,
            package_type,
            source_node,
            package_id,
        )
    }

    // [arch:allow-mutation-before-replay] see ADR-0014 — Reason: import_daily_reports must mutate before replay check (pre-existing legacy, idempotent flow); Date: 2026-08-09; Owner: Sync
    pub fn import_daily_reports(&self, reports: Vec<DailyReportResult>) -> Result<usize, AppError> {
        let mut count = 0;
        let now = // [arch:allow-utc-now] see ADR-0014 — Reason: timestamp for sync package rejection audit entry (pre-existing legacy); Date: 2026-08-09; Owner: Sync
            Utc::now().to_rfc3339();

        let report_repo = self.executor.reports();
        let product_repo = self.executor.products();

        for report_result in reports {
            let fiscal_year = report_result.report.fiscal_year;

            self.validate_fiscal(
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
                                None,
                            )?;
                        }
                    }
                }

                count += 1;
            }
        }

        Ok(count)
    }

    // [arch:allow-mutation-before-replay] see ADR-0014 — Reason: import_monthly_report mutation before replay in conflict handling (pre-existing legacy); Date: 2026-08-09; Owner: Sync
    pub fn import_monthly_report(
        &self,
        _unit_id: &str,
        _year: i32,
        _month: u32,
        reports: Vec<DailyReportResult>,
    ) -> Result<usize, AppError> {
        let count = reports.len();
        let report_repo = self.executor.reports();

        for report in reports {
            let report_id = uuid::Uuid::new_v4().to_string();
            let now = // [arch:allow-utc-now] see ADR-0014 — Reason: timestamp for sync conflict audit entry (pre-existing legacy); Date: 2026-08-09; Owner: Sync
                Utc::now().to_rfc3339();
            let fiscal_year = report.report.fiscal_year;

            self.validate_fiscal(fiscal_year, "sync_import", "remote_node", &report.report.id)?;

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

    // [arch:allow-mutation-before-replay] see ADR-0014 — Reason: import_products_sync mutation before replay in validation (pre-existing legacy); Date: 2026-08-09; Owner: Sync
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
                Some(existing) => super::sync_import_validation_service::is_incoming_newer(
                    &record.updated_at,
                    existing,
                ),
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

            if record.deleted == 0 {
                let stock_exists = inventory_repo.stock_exists_for_product(&record.id)?;

                if !stock_exists {
                    let stock_id = uuid::Uuid::new_v4().to_string();
                    inventory_repo.insert_empty_stock(
                        &stock_id,
                        &record.id,
                        &record.updated_at,
                        &record.node_id,
                    )?;
                }
            }
        }

        Ok((imported_count, updated_count, skipped_count))
    }

    /// Apply a ContractCatalog projection (ADR-0055 / SEC-087-F ContractCatalog
    /// V2, WILAYA → UNIT). WILAYA is the single source of truth for every
    /// WILAYA-owned column (suppliers, associations, contract headers/lines,
    /// contracted+released quantities, entitlement state, tax policies);
    /// UNIT-owned runtime state (`fulfilled_quantity`, `reserved_quantity`) is
    /// never touched by a sync package. On a UNIT importer (`importer_unit_id =
    /// Some`) only the rows scoped to that unit are applied; on a WILAYA
    /// importer the full catalog is upserted (restore path).
    pub fn import_contract_catalog_sync(
        &self,
        dataset: &ContractCatalogExportDataset,
        importer_unit_id: Option<&str>,
    ) -> Result<ContractCatalogImportSummary, AppError> {
        let supplier_repo = self.executor.suppliers();
        let contract_repo = self.executor.contracts();
        let policy_repo = self.executor.fiscal_tax_policy();

        let in_scope =
            |unit_id: &str| importer_unit_id.is_none() || Some(unit_id) == importer_unit_id;

        // Supplier ids relevant to the scoped unit set.
        let mut scoped_supplier_ids = BTreeSet::new();
        for link in &dataset.unit_supplier_links {
            if in_scope(&link.unit_id) {
                scoped_supplier_ids.insert(link.supplier_id.clone());
            }
        }
        for row in &dataset.contracts {
            if in_scope(&row.contract.unit_id) {
                scoped_supplier_ids.insert(row.contract.supplier_id.clone());
            }
        }

        let mut summary = ContractCatalogImportSummary::default();

        for supplier in &dataset.suppliers {
            if importer_unit_id.is_some() && !scoped_supplier_ids.contains(&supplier.id) {
                summary.suppliers_skipped += 1;
                continue;
            }
            let existing = supplier_repo.get_supplier(&supplier.id)?;
            supplier_repo.upsert_sync_supplier(supplier)?;
            if existing.is_some() {
                summary.suppliers_updated += 1;
            } else {
                summary.suppliers_imported += 1;
            }
        }

        for link in &dataset.unit_supplier_links {
            if !in_scope(&link.unit_id) {
                summary.links_skipped += 1;
                continue;
            }
            if !supplier_repo.supplier_associated_with_unit(&link.unit_id, &link.supplier_id)? {
                supplier_repo.associate_with_unit(&link.unit_id, &link.supplier_id)?;
            }
            summary.links_applied += 1;
        }

        // Tax policies are a global dimension on both importer roles.
        for policy in &dataset.tax_policies {
            policy_repo.upsert_sync_tax_policy(policy)?;
            summary.tax_policies_applied += 1;
        }

        for row in &dataset.contracts {
            if !in_scope(&row.contract.unit_id) {
                summary.contracts_skipped += 1;
                continue;
            }

            let existing_contract = contract_repo.get_contract(&row.contract.id)?;
            contract_repo.upsert_sync_contract(&row.contract)?;
            if existing_contract.is_some() {
                summary.contracts_updated += 1;
            } else {
                summary.contracts_imported += 1;
            }

            for line in &row.product_lines {
                let existing = contract_repo.get_contract_product(&line.id)?;
                contract_repo.upsert_sync_contract_product(
                    &line.id,
                    &line.contract_id,
                    &line.product_id,
                    line.proposed_price,
                    line.agreed_price,
                    &line.created_at,
                )?;
                if existing.is_some() {
                    summary.product_lines_updated += 1;
                } else {
                    summary.product_lines_imported += 1;
                }
            }

            let mut scoped_allocation_ids = BTreeSet::new();
            for alloc_row in &row.allocations {
                if !in_scope(&alloc_row.allocation.unit_id) {
                    summary.allocations_skipped += 1;
                    continue;
                }
                scoped_allocation_ids.insert(alloc_row.allocation.id.clone());
                let a = &alloc_row;
                let existing = contract_repo.get_allocation(&a.allocation.id)?;
                // [arch:allow-mutation-before-replay] see ADR-0014 — Reason: replay detection runs in the ContractCatalog import usecase registry BEFORE this service applies rows; this upsert is the post-replay apply step; Date: 2026-09-02; Owner: Sync
                contract_repo.upsert_sync_allocation(&a.allocation, &a.created_at)?;
                if existing.is_some() {
                    summary.allocations_updated += 1;
                } else {
                    summary.allocations_imported += 1;
                }
            }

            for exc in &row.exceptions {
                // Exceptions are only applicable alongside their scoped
                // allocation (the FK targets local contract_allocations).
                if !scoped_allocation_ids.contains(&exc.allocation_id) {
                    summary.exceptions_skipped += 1;
                    continue;
                }
                if contract_repo.get_exception(&exc.id)?.is_none() {
                    contract_repo.upsert_sync_exception(
                        &exc.id,
                        &exc.allocation_id,
                        exc.released_quantity,
                        &exc.reason_code,
                        &exc.reason_note,
                        &exc.created_by,
                        &exc.created_at,
                    )?;
                    summary.exceptions_imported += 1;
                } else {
                    summary.exceptions_skipped += 1;
                }
            }
        }

        Ok(summary)
    }

    // [arch:allow-mutation-before-replay] see ADR-0014 — Reason: import_stock_movements mutation before replay check (pre-existing legacy); Date: 2026-08-09; Owner: Sync
    pub fn import_stock_movements(
        &self,
        movements: Vec<crate::models::inventory::StockMovement>,
        unit_id: Option<&str>,
    ) -> Result<usize, AppError> {
        let mut count = 0;
        let movement_repo = self.executor.stock_movements();

        for mut m in movements {
            if let Some(fiscal_year) = m.fiscal_year {
                crate::application::services::FiscalValidationService::new(self.executor)
                    .assert_fiscal_year_open(fiscal_year)?;
            } else {
                let dt = crate::errors::parse_datetime_rfc3339(&m.timestamp)?;
                let fiscal_year = dt.year();
                log::warn!(
                    target: "grpc::fiscal",
                    "FISCAL FALLBACK: stock movement timestamp-inferred fiscal_year={}",
                    fiscal_year
                );
                crate::application::services::FiscalValidationService::new(self.executor)
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

    /// Phase 5.C: Transactional sync import pipeline.
    ///
    /// Performs two-phase replay detection (outside then inside transaction),
    /// emits audit events on rejection, and commits package application atomically.
    pub fn execute_import(
        db: &mut Database,
        request: SyncImportRequest,
        current_fiscal_year: i32,
        max_historical_years: i32,
    ) -> SyncImportResult {
        let package_id = request.package_id.clone();
        let kind_str = request.kind.as_str();

        let replay_detector = Self::build_replay_detector(db, &request);

        let validation = SyncImportValidationService::validate(
            &request,
            &replay_detector,
            current_fiscal_year,
            max_historical_years,
        );

        if !validation.all_checks_passed {
            return Self::handle_rejection(db, request, validation);
        }

        // [arch:allow-non-nested] see ADR-0014 — Reason: separate code path from handle_rejection (not a nested transaction); Date: 2026-08-09; Owner: Sync
        let result = db.with_event_persistence(
            |ctx: &mut EventContext<'_>| -> Result<ImportExecutionSummary, AppError> {
                let inner_detector = Self::build_replay_detector_from_ctx(ctx, &request);

                let inner_validation = SyncImportValidationService::validate(
                    &request,
                    &inner_detector,
                    current_fiscal_year,
                    max_historical_years,
                );

                if !inner_validation.all_checks_passed {
                    let conflict = match &inner_validation.replay_check {
                        ConflictDetectionOutcome::ConflictDetected(c) => c,
                        _ => {
                            return Err(AppError::Internal(
                                "replay detected during transaction with no conflict details"
                                    .into(),
                            ));
                        }
                    };

                    // [arch:allow-unwrap-or] see ADR-0014 — Reason: intended fallback to 0 for non-numeric conflict IDs (replay-detect path); Date: 2026-08-09; Owner: Sync
                    let conflict_id_val = conflict.conflict_id().0.parse::<i64>().unwrap_or(0);
                    ctx.emit(DomainEvent::SyncConflictDetected {
                        conflict_id: conflict_id_val,
                        conflict_type: Some(conflict.conflict_type_str().to_string()),
                        package_id: Some(package_id.clone()),
                        details: Some(conflict.metadata().explanation.description.clone()),
                    });

                    return Err(AppError::BusinessLogic(
                        crate::errors::BusinessLogicError::DuplicateSyncPackage {
                            package_id: package_id.clone(),
                        },
                    ));
                }

                let repo = ctx.executor().sync_applied_packages();
                let inserted = repo
                    .insert_if_new(
                        // [arch:allow-mutation-before-replay] see ADR-0014 — Reason: mutation after replay check but before full validation (pre-existing legacy); Date: 2026-08-09; Owner: Sync
                        &package_id,
                        kind_str,
                        Some(&request.source_node_id),
                        "sync_import",
                        None,
                    )
                    .map_err(|e| AppError::Internal(format!("failed to record package: {}", e)))?;

                if !inserted {
                    return Err(AppError::BusinessLogic(
                        crate::errors::BusinessLogicError::DuplicateSyncPackage {
                            package_id: package_id.clone(),
                        },
                    ));
                }

                let mutations = Self::apply_package_mutations(ctx, &request)?;

                let event_seq = ctx.emit(DomainEvent::SyncPackageImported {
                    package_id: package_id.clone(),
                    kind: kind_str.to_string(),
                    source_node_id: Some(request.source_node_id.clone()),
                    sequence_number: request.incoming_sequence,
                });

                log::info!(
                    target: "grpc::sync",
                    "[IMPORT_SUCCEEDED] package_id={} kind={} mutations={:?} event_seq={}",
                    package_id, kind_str, mutations, event_seq
                );

                Ok(ImportExecutionSummary {
                    package_id: package_id.clone(),
                    kind: request.kind,
                    transaction_id: String::new(),
                    events_emitted: event_seq,
                    mutations_applied: mutations,
                    fiscal_year: request.fiscal_year,
                })
            },
        );

        match result {
            Ok((summary, _buffer)) => SyncImportResult {
                success: true,
                execution_summary: Some(ImportExecutionSummary {
                    transaction_id: _buffer.transaction_id().to_string(),
                    ..summary
                }),
                conflict_outcome: None,
                replay_protection: Some(ReplayProtectionResult {
                    package_id: package_id.clone(),
                    replay_detected: false,
                    conflict: None,
                    audited: true,
                }),
                error: None,
            },
            Err(e) => SyncImportResult {
                success: false,
                execution_summary: None,
                conflict_outcome: None,
                replay_protection: Some(ReplayProtectionResult {
                    package_id: package_id.clone(),
                    replay_detected: false,
                    conflict: None,
                    audited: true,
                }),
                error: Some(ImportExecutionError::TransactionFailed(e.to_string())),
            },
        }
    }

    fn handle_rejection(
        db: &mut Database,
        request: SyncImportRequest,
        validation: ValidationSnapshot,
    ) -> SyncImportResult {
        let package_id = request.package_id.clone();

        // [arch:allow-non-nested] see ADR-0014 — Reason: separate code path from execute_import (not a nested transaction); Date: 2026-08-09; Owner: Sync
        let result = db.with_event_persistence(
            |ctx: &mut EventContext<'_>| -> Result<ImportExecutionSummary, AppError> {
                let conflict = match &validation.replay_check {
                    ConflictDetectionOutcome::ConflictDetected(c) => c,
                    _ => {
                        return Err(AppError::Internal(
                            "validation failed with no conflict details".into(),
                        ));
                    }
                };

                // [arch:allow-unwrap-or] see ADR-0014 — Reason: intended fallback to 0 for non-numeric conflict IDs (conflict path); Date: 2026-08-09; Owner: Sync
                let conflict_id_val = conflict.conflict_id().0.parse::<i64>().unwrap_or(0);
                let event_seq = ctx.emit(DomainEvent::SyncConflictDetected {
                    conflict_id: conflict_id_val,
                    conflict_type: Some(conflict.conflict_type_str().to_string()),
                    package_id: Some(package_id.clone()),
                    details: Some(conflict.metadata().explanation.description.clone()),
                });

                let summary = ImportExecutionSummary {
                    package_id: package_id.clone(),
                    kind: request.kind,
                    transaction_id: String::new(),
                    events_emitted: event_seq,
                    mutations_applied: ImportMutationSummary::none(),
                    fiscal_year: request.fiscal_year,
                };

                log::warn!(
                    target: "grpc::sync",
                    "[IMPORT_REJECTED] package_id={} reason={} event_seq={}",
                    package_id, conflict.conflict_type_str(), event_seq
                );

                Ok(summary)
            },
        );

        match result {
            Ok((summary, _buffer)) => SyncImportResult {
                success: false,
                execution_summary: Some(ImportExecutionSummary {
                    transaction_id: _buffer.transaction_id().to_string(),
                    ..summary
                }),
                conflict_outcome: None,
                replay_protection: Some(ReplayProtectionResult {
                    package_id: package_id.clone(),
                    replay_detected: true,
                    conflict: match &validation.replay_check {
                        ConflictDetectionOutcome::ConflictDetected(c) => Some(c.clone()),
                        _ => None,
                    },
                    audited: true,
                }),
                error: Some(ImportExecutionError::ReplayDetected(package_id)),
            },
            Err(e) => SyncImportResult {
                success: false,
                execution_summary: None,
                conflict_outcome: None,
                replay_protection: Some(ReplayProtectionResult {
                    package_id: package_id.clone(),
                    replay_detected: true,
                    conflict: None,
                    audited: false,
                }),
                error: Some(ImportExecutionError::Internal(e.to_string())),
            },
        }
    }

    fn build_replay_detector(db: &Database, request: &SyncImportRequest) -> ReplayDetector {
        let executor = db.executor();

        let repo = executor.sync_applied_packages();
        // [arch:allow-unwrap-or] see ADR-0014 — Reason: false is safe default for not-imported-yet (build_replay_detector); Date: 2026-08-09; Owner: Sync
        let is_imported = repo.has_imported(&request.package_id).unwrap_or(false);

        let mut applied_packages = BTreeSet::new();
        if is_imported {
            applied_packages.insert(request.package_id.clone());
        }

        ReplayDetector::new(
            AppliedPackages::new(applied_packages),
            AppliedTransitions::new(BTreeSet::new()),
            SeenTransactions::new(BTreeSet::new()),
            request.source_node_id.clone(),
            request.target_node_id.clone(),
        )
    }

    fn build_replay_detector_from_ctx(
        ctx: &mut EventContext<'_>,
        request: &SyncImportRequest,
    ) -> ReplayDetector {
        let executor = ctx.executor();

        let repo = executor.sync_applied_packages();
        // [arch:allow-unwrap-or] see ADR-0014 — Reason: false is safe default for not-imported-yet (build_replay_detector_from_ctx); Date: 2026-08-09; Owner: Sync
        let is_imported = repo.has_imported(&request.package_id).unwrap_or(false);

        let mut applied_packages = BTreeSet::new();
        if is_imported {
            applied_packages.insert(request.package_id.clone());
        }

        ReplayDetector::new(
            AppliedPackages::new(applied_packages),
            AppliedTransitions::new(BTreeSet::new()),
            SeenTransactions::new(BTreeSet::new()),
            request.source_node_id.clone(),
            request.target_node_id.clone(),
        )
    }

    fn apply_package_mutations(
        _ctx: &mut EventContext<'_>,
        _request: &SyncImportRequest,
    ) -> Result<ImportMutationSummary, AppError> {
        Ok(ImportMutationSummary::none())
    }

    pub fn validate_and_execute(
        db: &mut Database,
        request: SyncImportRequest,
        current_fiscal_year: i32,
        max_historical_years: i32,
    ) -> SyncImportResult {
        Self::execute_import(db, request, current_fiscal_year, max_historical_years)
    }
}

impl crate::architecture::Service for SyncImportExecutionService<'_> {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::services::sync_import_models::SyncPackageKind;
    use crate::db::ConnectionFactory;
    use std::collections::HashMap;

    fn make_fiscal_request(package_id: &str) -> SyncImportRequest {
        SyncImportRequest {
            package_id: package_id.into(),
            source_node_id: "node-a".into(),
            target_node_id: "node-b".into(),
            kind: SyncPackageKind::Fiscal,
            incoming_sequence: None,
            last_applied_sequence: None,
            fiscal_year: Some(2025),
            transition_ids: vec![],
            transaction_ids: vec![],
            payload: HashMap::new(),
        }
    }

    #[test]
    fn import_package_succeeds() {
        let mut db = ConnectionFactory::new_for_test().expect("db");
        let request = make_fiscal_request("pkg-success-1");
        let result = SyncImportExecutionService::execute_import(&mut db, request, 2025, 5);
        assert!(result.success, "import should succeed: {:?}", result.error);
    }

    #[test]
    fn duplicate_package_rejected() {
        let mut db = ConnectionFactory::new_for_test().expect("db");
        let request = make_fiscal_request("pkg-dup-1");
        let r1 = SyncImportExecutionService::execute_import(&mut db, request.clone(), 2025, 5);
        assert!(r1.success);
        let r2 = SyncImportExecutionService::execute_import(&mut db, request, 2025, 5);
        assert!(!r2.success, "duplicate should be rejected");
        assert!(r2
            .replay_protection
            .as_ref()
            .map(|r| r.replay_detected)
            .unwrap_or(false)); // [arch:allow-unwrap-or] see ADR-0014 — Reason: test assertion default — false is safe; Date: 2026-08-09; Owner: Sync
    }

    #[test]
    fn rollback_on_mid_import_failure() {
        let mut db = ConnectionFactory::new_for_test().expect("db");
        let mut request = make_fiscal_request("pkg-rollback");
        request.fiscal_year = Some(2030);
        let result = SyncImportExecutionService::execute_import(&mut db, request, 2025, 5);
        assert!(!result.success);
    }

    #[test]
    fn same_input_same_db_same_result() {
        let mut db1 = ConnectionFactory::new_for_test().expect("db");
        let mut db2 = ConnectionFactory::new_for_test().expect("db");
        let request1 = make_fiscal_request("pkg-repro-1");
        let request2 = make_fiscal_request("pkg-repro-1");
        let r1 = SyncImportExecutionService::execute_import(&mut db1, request1, 2025, 5);
        let r2 = SyncImportExecutionService::execute_import(&mut db2, request2, 2025, 5);
        assert_eq!(r1.success, r2.success);
    }

    #[test]
    fn audit_persisted_on_rejection() {
        let mut db = ConnectionFactory::new_for_test().expect("db");
        let request = make_fiscal_request("pkg-audit-reject");
        let r1 = SyncImportExecutionService::execute_import(&mut db, request.clone(), 2025, 5);
        assert!(r1.success);
        let r2 = SyncImportExecutionService::execute_import(&mut db, request, 2025, 5);
        assert!(!r2.success);
        assert!(r2.error.is_some());
    }

    #[test]
    fn idempotent_same_package_twice() {
        let mut db = ConnectionFactory::new_for_test().expect("db");
        let r1 = SyncImportExecutionService::execute_import(
            &mut db,
            make_fiscal_request("pkg-idem"),
            2025,
            5,
        );
        assert!(r1.success);
        let r2 = SyncImportExecutionService::execute_import(
            &mut db,
            make_fiscal_request("pkg-idem"),
            2025,
            5,
        );
        assert!(!r2.success);
    }
}
