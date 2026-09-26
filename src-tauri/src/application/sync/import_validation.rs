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
