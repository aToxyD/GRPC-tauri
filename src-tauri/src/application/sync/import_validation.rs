//! Import-side semantic validation for interchange packages.

use chrono::Datelike;

use crate::application::sync::SyncPackage;
use crate::application::usecases::exports::types::{
    ContractCatalogExportDataset, DailyReportExportDataset, FulfillmentFactExportDataset,
    MonthlySummaryExportDataset, ProductsExportDataset, FULFILLMENT_FACT_VERSION,
};
use crate::domain::validation;
use crate::errors::{AppError, AppResult, BusinessLogicError, ValidationError};
use crate::infrastructure::sync::packages::content_package_id::derive_content_package_id;
use crate::repositories::{DbExecutor, RepositoryProvider};

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

/// SEC-087 Phase 6B (ADR-0057 §3.4): Product V3 configuration gate.
/// All four fields (purchase_unit, consumption_unit, conversion_factor,
/// tva_classification) are REQUIRED; the domain validator rejects any record
/// whose configuration is absent or out-of-range. This function is invoked at
/// the TOP of `import_products_package::execute` BEFORE `has_imported` and
/// before any Product/inventory row mutation — a fail-closed validation-first
/// boundary. The package-level gate (enforced by the central pipeline)
/// guarantees the payload carries the four fields at the kind/version level;
/// here we enforce the SEMANTIC invariant (valid unit codes, factor > 0,
/// same-unit factor = 1, etc.) per record.
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

    // SEC-087 Phase 6B (ADR-0057 §3.4): per-record V3 configuration
    // validation — fail-closed; a single malformed record rejects the entire
    // package BEFORE any mutation.
    for row in &package.payload.product_rows {
        validation::validate_product_units(
            row.purchase_unit,
            row.consumption_unit,
            row.conversion_factor,
            row.tva_classification,
        )
        .map_err(|e| {
            AppError::Validation(ValidationError::InvalidFormat {
                field: format!("product_rows[{}].config", row.product.id),
                message: format!(
                    "Product «{}» V3 config rejected: {}",
                    row.product.id,
                    match &e {
                        AppError::Validation(v) => format!("{v}"),
                        other => format!("{other}"),
                    }
                ),
            })
        })?;
    }

    Ok(())
}

/// ADR-0058: Product unit/factor tuple immutability gate (DB-backed).
///
/// Once a Product has any `stock_movements` row (`has_stock_activity`), its
/// `(purchase_unit, consumption_unit, conversion_factor)` tuple is frozen.
/// This gate runs inside the existing Product V3 validation/import boundary —
/// AFTER the pure `validate_products_package_for_import` gate and BEFORE any
/// Product/inventory mutation or imported-package marking. A forbidden tuple
/// change rejects the ENTIRE package, fail-closed, with zero mutation.
///
/// Non-goals preserved (ADR-0058 §4): `name`, `base_price`, and
/// `tva_classification` are NOT compared here — `tva_classification` remains
/// governed by the fiscal-year rule (ADR-0055 §3.9).
pub fn validate_products_package_unit_config_immutable(
    package: &SyncPackage<ProductsExportDataset>,
    executor: DbExecutor<'_>,
) -> AppResult<()> {
    for row in &package.payload.product_rows {
        let product_id = &row.product.id;
        if !executor.stock_movements().has_stock_activity(product_id)? {
            continue;
        }
        let persisted = executor.products().get_product_sync_config(product_id)?;
        if let Some(p) = persisted {
            let incoming = (
                row.purchase_unit,
                row.consumption_unit,
                row.conversion_factor,
            );
            let frozen = (
                Some(p.purchase_unit),
                Some(p.consumption_unit),
                Some(p.conversion_factor),
            );
            if incoming != frozen {
                return Err(AppError::BusinessLogic(
                    BusinessLogicError::UnitConfigImmutableAfterMovement {
                        product_id: product_id.clone(),
                    },
                ));
            }
        }
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

/// Q4 (ADR-0064 §4.7 D5, §9.3.3): allocation/contract unit consistency for a
/// Contract Catalog import.
///
/// The single owner of the allocation-ownership rule. Two invariants:
///
/// 1. **Contract-scoped (always).** An allocation's own `unit_id` MUST equal
///    the `unit_id` of its parent contract. The exporter's SQL boundary
///    (`list_sync_allocations_for_contract_unit`) already guarantees this for
///    every artifact it emits (proved by the cross-unit exclusion probe
///    `alloc-a1-b`), so a divergence in a received package is a structural
///    inconsistency the schema alone cannot prevent (G26).
/// 2. **Local-scope (UNIT importer only).** On a UNIT importer
///    (`local_unit_id = Some`) an allocation's `unit_id` MUST equal the local
///    `units.id`. A UNIT-scoped `UnitDistribution` artifact carries only the
///    target unit's rows, so any other unit is an explicit conflict. On the
///    WILAYA restore path (`local_unit_id = None`) the catalog legitimately
///    carries many units and only invariant 1 applies.
///
/// Fails closed with an explicit validation error — it is **never** a silent
/// skip (Q4). Callers run it BEFORE any catalog row is applied, so a rejected
/// package performs no partial write inside its transaction.
pub fn validate_contract_catalog_allocation_unit_consistency(
    allocation: &crate::models::ContractAllocation,
    contract_unit_id: &str,
    local_unit_id: Option<&str>,
) -> AppResult<()> {
    if allocation.unit_id != contract_unit_id {
        return Err(AppError::Validation(ValidationError::InvalidFormat {
            field: "contracts[].allocations[].unit_id".into(),
            message: format!(
                "التخصيص «{}» لا ينتمي إلى عقد «{}» — وحدة التخصيص «{}» تخالف وحدة العقد «{}»",
                allocation.id, allocation.contract_id, allocation.unit_id, contract_unit_id
            ),
        }));
    }
    if let Some(local) = local_unit_id {
        if allocation.unit_id != local {
            return Err(AppError::Validation(ValidationError::InvalidFormat {
                field: "contracts[].allocations[].unit_id".into(),
                message: format!(
                    "التخصيص «{}» لا يخص الوحدة المحلية — وحدة التخصيص «{}» تخالف نطاق الوحدة «{}»",
                    allocation.id, allocation.unit_id, local
                ),
            }));
        }
    }
    Ok(())
}

/// Dataset-level application of
/// [`validate_contract_catalog_allocation_unit_consistency`] over every
/// allocation row of a Contract Catalog package.
///
/// Runs on the UNIT import path after the importer's own unit is resolved and
/// before any row is applied (ADR-0064 Q4). The WILAYA restore path passes
/// `local_unit_id = None` so only the contract-scoped invariant is enforced.
pub fn validate_contract_catalog_allocation_units_for_import(
    package: &SyncPackage<ContractCatalogExportDataset>,
    local_unit_id: Option<&str>,
) -> AppResult<()> {
    for row in &package.payload.contracts {
        for alloc_row in &row.allocations {
            validate_contract_catalog_allocation_unit_consistency(
                &alloc_row.allocation,
                &row.contract.unit_id,
                local_unit_id,
            )?;
        }
    }
    Ok(())
}

/// Wire + semantic checks for a ContractCatalog package (ADR-0055 /
/// SEC-087-F). The catalog is a WILAYA-authoritative projection; a package
/// without contracts carries no projection and is rejected fail-closed
/// (mirrors the products validator's non-empty rule).
///
/// SEC-087 Phase 2 C4 (ADR-0059 §10, amended by ADR-0060): a
/// `UnitDistribution` package — the ONLY Contract Catalog export form — is
/// the unit-scoped per-unit projection and MAY legitimately carry an EMPTY
/// contract set (the target's own view contains no contracts), so the
/// non-empty rule applies ONLY outside `UnitDistribution`. Scope/target
/// binding itself is enforced separately by the importer.
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
    let is_unit_distribution = matches!(
        package.metadata.export_mode,
        Some(crate::application::sync::PackageExportMode::UnitDistribution)
    );
    if package.payload.contracts.is_empty() && !is_unit_distribution {
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

/// Wire + identity checks for an ADR-0061 `contract_fulfillment` package.
///
/// This is layer 2 of the four-layer idempotency model. The `package_id` of a
/// fulfillment package is **content-derived** — `hex(sha256(canonical_json(
/// dataset)))` — so re-exporting an unchanged state set produces the same
/// identity and is recognised as a duplicate instead of appearing as a new fact
/// stream. That contract is only meaningful if the destination verifies it, so
/// a `package_id` that does not describe its own payload is rejected
/// fail-closed, BEFORE any DB access.
///
/// `fact_version` is checked in the deserializer (where the untrusted bytes
/// first materialize); it is re-asserted here so the invariant holds for every
/// caller, including tests that build a package value directly.
pub fn validate_contract_fulfillment_package_for_import(
    package: &SyncPackage<FulfillmentFactExportDataset>,
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
    if package.payload.fact_version != FULFILLMENT_FACT_VERSION {
        return Err(AppError::Validation(ValidationError::InvalidFormat {
            field: "fact_version".into(),
            message: format!(
                "إصدار عقد حقيقة التنفيذ غير مدعوم: {} — المتوقع {}",
                package.payload.fact_version, FULFILLMENT_FACT_VERSION
            ),
        }));
    }
    if package.payload.facts.is_empty() {
        return Err(AppError::Validation(ValidationError::InvalidFormat {
            field: "facts".into(),
            message: "الحزمة لا تحتوي حقائق تنفيذ — لا يمكن تطبيقها".into(),
        }));
    }

    let expected_id = derive_content_package_id(&package.payload)?;
    if expected_id.0 != package.metadata.package_id.0 {
        return Err(AppError::Validation(ValidationError::InvalidFormat {
            field: "package_id".into(),
            message: format!(
                "معرّف الحزمة لا يطابق محتواها (المتوقع {}) — رفضت الحزمة",
                expected_id.0
            ),
        }));
    }
    Ok(())
}
