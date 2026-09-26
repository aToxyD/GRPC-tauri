//! Apply an ADR-0061 `contract_fulfillment` package on a WILAYA node.
//!
//! The package carries the source UNIT's **complete current cumulative
//! fulfillment state set** — one absolute snapshot per eligible
//! `ContractAllocation`, never a per-order event and never a delta. This
//! use case is the consumer half of the four-layer idempotency model
//! (ADR-0061 §4):
//!
//! 1. payload schema version — `fact_version`, enforced fail-closed;
//! 2. content-derived `package_id`;
//! 3. package replay protection — the same content identity is the
//!    `applied_sync_packages` primary key;
//! 4. per-fact guarded monotone state convergence — this file.
//!
//! Layers 1 and 2 are enforced in the **reader** — the deserializer entry
//! point `contract_fulfillment_from_reader`, reached through
//! `read_contract_fulfillment_package_from_file` — BEFORE any database access.
//! The application layer re-verifies both for use-case callers that bypass the
//! reader: [`validate_contract_fulfillment_package_for_import`] re-asserts
//! `fact_version` and recomputes the content identity from the dataset. Layer 3
//! is owned by the shared import pipeline. Only layer 4 lives here.
//!
//! ## Two passes, one transaction
//!
//! Every fact is resolved and fully validated BEFORE the first mutation, then
//! all facts are applied inside the caller's transaction (`run_import_pipeline`
//! owns the single boundary — freeze §2.7 forbids nested transactions). One
//! invalid fact therefore rejects the ENTIRE package with no partial effect.
//!
//! ## Monotone convergence, not causal attribution
//!
//! `contract_allocations.fulfilled_quantity` is the only persisted
//! authoritative value, so the destination can only ever record "the source's
//! current cumulative state for this allocation" — never "how this state was
//! reached" (ADR-0061 §3). A fact whose absolute value is not greater than the
//! local state is a no-op (already satisfied or stale); it is never applied and
//! never decreases local state, which makes the outcome independent of delivery
//! order and duplicate delivery.

use crate::application::sync::import_validation::validate_contract_fulfillment_package_for_import;
use crate::application::sync::{source_id_allowed_for_unit, ImportedPackageRegistry, SyncPackage};
use crate::application::usecases::exports::types::FulfillmentFactExportDataset;
use crate::domain::numeric::legacy_float::quantity_from_f64;
use crate::domain::numeric::{NumericError, Quantity};
use crate::errors::{AppError, AppResult, BusinessLogicError, ValidationError};
use crate::repositories::contracts::FulfillmentResolutionRow;
use crate::repositories::{DbExecutor, RepositoryProvider};

pub const CONTRACT_FULFILLMENT_PACKAGE_KIND: &str = "contract_fulfillment";

#[derive(Debug, Clone)]
pub struct ImportContractFulfillmentPackageInput {
    pub package: SyncPackage<FulfillmentFactExportDataset>,
    pub unit_id: String,
    pub importer_wilaya_code: String,
    pub imported_by: String,
}

#[derive(Debug, Clone)]
pub struct ImportContractFulfillmentPackageOutcome {
    /// Facts whose absolute state was strictly newer and was written.
    pub applied_count: usize,
    /// Facts already satisfied (or stale) at the destination: verified no-ops.
    pub already_satisfied_count: usize,
    pub unit_id: String,
    pub package_id: String,
}

/// Per-fact disposition decided in the validation pass.
#[derive(Debug)]
enum Disposition {
    /// The fact is strictly newer than the local state and passed the upper
    /// guard pre-check — apply it.
    Apply,
    /// The local state already satisfies (or exceeds) the fact. Deliberately a
    /// no-op: this is what makes duplicate and out-of-order delivery
    /// indistinguishable from a single delivery.
    AlreadySatisfied,
}

pub fn execute(
    executor: DbExecutor<'_>,
    registry: &impl ImportedPackageRegistry,
    input: ImportContractFulfillmentPackageInput,
) -> AppResult<ImportContractFulfillmentPackageOutcome> {
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

    // 1. Wire + identity gate (package_id present, source node present,
    //    content-derived package_id matches the dataset, fact_version known).
    validate_contract_fulfillment_package_for_import(&input.package)?;

    // 2. Source unit resolution and cross-wilaya containment.
    let unit_row = executor.units().get_unit(unit_trim)?.ok_or_else(|| {
        AppError::BusinessLogic(BusinessLogicError::ResourceNotFound {
            resource: "الوحدة".into(),
            id: unit_trim.to_string(),
        })
    })?;
    if unit_row.wilaya_code != importer_wilaya_trim {
        return Err(AppError::BusinessLogic(
            BusinessLogicError::CrossWilayaForbidden {
                resource: "تنفيذ_تخصيصات_عقد_sync".into(),
                from_wilaya: unit_row.wilaya_code.clone(),
                to_wilaya: importer_wilaya_trim.to_string(),
            },
        ));
    }

    // 3. Source node authorization check.
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

    // 4. Replay: exact `package_id` dedup. Re-exporting an unchanged state set
    //    derives the same content `package_id`, so a genuine repeat is rejected
    //    here rather than being silently re-applied (ADR-0061 §4).
    let package_id = input.package.metadata.package_id.clone();
    if registry.has_imported(&package_id)? {
        return Err(AppError::BusinessLogic(
            BusinessLogicError::DuplicateSyncPackage {
                package_id: package_id.0.clone(),
            },
        ));
    }

    // 5. Validation pass — resolve and validate EVERY fact before any write.
    let contracts = executor.contracts();
    let mut planned: Vec<Disposition> = Vec::with_capacity(input.package.payload.facts.len());
    let mut seen_allocation_ids: Vec<&str> = Vec::with_capacity(input.package.payload.facts.len());

    for fact in &input.package.payload.facts {
        let allocation_id = fact.allocation_id.trim();
        if allocation_id.is_empty() {
            return Err(AppError::Validation(ValidationError::Required {
                field: "allocation_id".into(),
            }));
        }
        // A repeated identity inside one package is ambiguous (which absolute
        // value wins?) — fail closed instead of picking an order-dependent one.
        if seen_allocation_ids.contains(&allocation_id) {
            return Err(AppError::Validation(ValidationError::InvalidFormat {
                field: "allocation_id".into(),
                message: format!("معرّف تخصيص مكرر داخل الحزمة: «{allocation_id}»"),
            }));
        }
        seen_allocation_ids.push(allocation_id);

        let resolution = contracts
            .resolve_allocation_for_fulfillment_sync(allocation_id)?
            .ok_or_else(|| {
                AppError::BusinessLogic(BusinessLogicError::ResourceNotFound {
                    resource: "تخصيص العقد".into(),
                    id: allocation_id.to_string(),
                })
            })?;

        // Ownership proof: the fact carries no `unit_id` by design (ADR-0061
        // §3), so the allocation's OWNING unit column is the only authority.
        // Without this, any authenticated UNIT of the same WILAYA could name
        // another unit's allocation and drive its fulfilled state.
        if resolution.unit_id.trim() != unit_trim {
            return Err(AppError::Validation(ValidationError::InvalidFormat {
                field: "allocation_id".into(),
                message: format!("التخصيص «{allocation_id}» لا يخص الوحدة «{unit_trim}»"),
            }));
        }

        planned.push(validate_fact(fact, &resolution)?);
    }

    // 6. Application pass — same transaction, monotone guarded absolute SET.
    let mut applied_count = 0usize;
    let mut already_satisfied_count = 0usize;
    for (fact, disposition) in input.package.payload.facts.iter().zip(planned) {
        let allocation_id = fact.allocation_id.trim();
        match disposition {
            Disposition::AlreadySatisfied => already_satisfied_count += 1,
            Disposition::Apply => {
                let changed = contracts
                    .set_fulfilled_absolute_guarded(allocation_id, fact.fulfilled_quantity)?;
                if changed == 0 {
                    // The validation pass proved this fact is applicable, so a
                    // zero-row update means the state moved underneath us or the
                    // guard disagrees with the pre-check. Reject the WHOLE
                    // package instead of recording a partial import.
                    return Err(AppError::BusinessLogic(
                        BusinessLogicError::ValidationError {
                            field: "allocation_id".into(),
                            message: format!(
                                "تعذّر تطبيق حالة تنفيذ التخصيص «{allocation_id}» — تغيير غير متوقع في الكمية"
                            ),
                        },
                    ));
                }
                applied_count += 1;
            }
        }
    }

    // 7. Mark the package as imported (same transaction).
    registry.mark_imported(&package_id)?;

    Ok(ImportContractFulfillmentPackageOutcome {
        applied_count,
        already_satisfied_count,
        unit_id: unit_trim.to_string(),
        package_id: package_id.0.clone(),
    })
}

/// Decide a fact's disposition, rejecting everything that is not provably
/// applicable, before any mutation happens.
fn validate_fact(
    fact: &crate::application::usecases::exports::types::FulfillmentFact,
    local: &FulfillmentResolutionRow,
) -> AppResult<Disposition> {
    let allocation_id = fact.allocation_id.trim();

    // Fiscal year is a CROSS-CHECK, never an application input: an older
    // allocation whose fulfillment advanced after the fiscal year closed is
    // still legitimate and MUST keep converging (ADR-0061 §5).
    if fact.fiscal_year != local.fiscal_year {
        return Err(AppError::Validation(ValidationError::InvalidFormat {
            field: "fiscal_year".into(),
            message: format!(
                "السنة المالية للحقيقة ({}) لا تطابق التخصيص «{allocation_id}» ({})",
                fact.fiscal_year, local.fiscal_year
            ),
        }));
    }

    // Unit triple cross-check: the payload carries no authority, it only proves
    // the fact was produced against the same contract product the destination
    // holds. `None` is treated as factor 1, consistent with
    // `OrderUnitSnapshot::effective_factor`.
    if fact.purchase_unit != local.purchase_unit
        || fact.conversion_factor.unwrap_or(1) != local.conversion_factor.unwrap_or(1)
    {
        return Err(AppError::Validation(ValidationError::InvalidFormat {
            field: "purchase_unit".into(),
            message: format!(
                "وحدة الشراء/معامل التحويل للحقيقة لا تطابق عقد المنتج المحلي للتخصيص «{allocation_id}»"
            ),
        }));
    }

    // Exact decimal arithmetic (scale 3) — the same value space the SQL guard
    // compares in, so the pre-check and the persisted guard cannot disagree.
    let absolute = quantity(fact.fulfilled_quantity, allocation_id)?;
    let current = quantity(local.fulfilled_quantity, allocation_id)?;

    // Monotone convergence: an equal or older absolute state is a no-op, never
    // a decrease (ADR-0061 §4).
    if current >= absolute {
        return Ok(Disposition::AlreadySatisfied);
    }

    // Upper guard pre-check. The persisted guard is
    // `fulfilled + released + reserved <= contracted`; rejecting here turns an
    // otherwise opaque zero-row update into an actionable error, and (because
    // the whole package is rejected) leaves the destination untouched.
    let projected = absolute
        .checked_add(quantity(local.released_quantity, allocation_id)?)
        .and_then(|q| {
            quantity(local.reserved_quantity, allocation_id)
                .map_err(|_| NumericError::Parse)
                .and_then(|reserved| q.checked_add(reserved))
        })
        .map_err(|_| {
            AppError::Validation(ValidationError::InvalidFormat {
                field: "fulfilled_quantity".into(),
                message: format!("تجاوز كمية التخصيص «{allocation_id}» الحد الرقمي المسموح"),
            })
        })?;
    let contracted = quantity(local.contracted_quantity, allocation_id)?;
    if projected > contracted {
        return Err(AppError::Validation(ValidationError::InvalidFormat {
            field: "fulfilled_quantity".into(),
            message: format!(
                "كمية التنفيذ ({}) مع المحرر/المحجوز تتجاوز الكمية التعاقدية ({}) للتخصيص «{allocation_id}»",
                fact.fulfilled_quantity, local.contracted_quantity
            ),
        }));
    }

    Ok(Disposition::Apply)
}

/// Wire `f64` → exact `Quantity` (scale 3), rejecting NaN/∞/negative values.
fn quantity(value: f64, allocation_id: &str) -> AppResult<Quantity> {
    quantity_from_f64(value).map_err(|e| {
        AppError::Validation(ValidationError::InvalidFormat {
            field: "fulfilled_quantity".into(),
            message: format!("كمية غير صالحة للتخصيص «{allocation_id}»: {e}"),
        })
    })
}
