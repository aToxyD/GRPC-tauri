//! Build the WILAYA-authoritative ContractCatalog dataset (ADR-0055 /
//! SEC-087-F sync ContractCatalog V2, WILAYA → UNIT).
//!
//! The payload is a read-only projection: suppliers, UNIT↔supplier
//! associations, contracts (headers + product lines + per-UNIT allocations +
//! allocation release exceptions), and the fiscal-year TVA policies. UNIT
//! importers apply ONLY the rows scoped to their own unit id.
//!
//! ADR-0059: the mode-carrying [`ExportContractCatalogInput`] selects either
//! the fleet-wide `FleetRestore` projection (ALL statuses, everything) or a
//! UNIT-scoped `UnitDistribution` projection:
//!
//! - contracts are selected at the SQL boundary by the target unit's internal
//!   `units.id` with the per-UNIT status policy (`accepted | active | ended`,
//!   `proposed | cancelled` excluded; an `Ended` contract of another UNIT can
//!   never be selected because its `unit_id` differs — unit scope trumps Ended
//!   eligibility, ADR-0059 §6);
//! - allocations/exceptions ride the selected contract chain;
//! - suppliers and unit–supplier links are restricted to the target unit's
//!   relationships (ADR-0059 §11.1);
//! - `tax_policies` remain GLOBAL (ADR-0059 §11.2);
//! - an empty contracts array is legitimate for a UNIT scoped artifact
//!   (ADR-0059 §10).
//!
//! Producer validation (§15.1) runs against the EXACT dataset selected for the
//! mode, before serialization/signing — B-only defects can never invalidate
//! A's artifact because B rows are never part of the validated dataset.

use std::collections::HashSet;

use crate::application::services::transport_target;
use crate::application::usecases::exports::types::{
    ContractCatalogAllocationRow, ContractCatalogContractRow, ContractCatalogExportDataset,
    ContractCatalogProductLine, ContractCatalogUnitSupplierLink, ExportContractCatalogInput,
    ExportContractCatalogMode,
};
use crate::errors::{AppError, AppResult, ValidationError};
use crate::repositories::{
    ContractRepository, DbExecutor, FiscalYearTaxPolicyRepository, RepositoryProvider,
    SupplierRepository,
};

pub fn execute(
    executor: DbExecutor<'_>,
    input: ExportContractCatalogInput,
) -> AppResult<ContractCatalogExportDataset> {
    let dataset = match input.mode {
        ExportContractCatalogMode::FleetRestore => {
            execute_fleet(executor, &ExportContractCatalogMode::FleetRestore)?
        }
        ExportContractCatalogMode::UnitDistribution { target_unit_code } => {
            execute_for_unit(executor, &target_unit_code)?
        }
    };
    Ok(dataset)
}

/// Fleet-wide projection (ADR-0059 §7): ALL statuses retained, ALL units.
/// Existing fleet semantics preserved unchanged.
fn execute_fleet(
    executor: DbExecutor<'_>,
    mode: &ExportContractCatalogMode,
) -> AppResult<ContractCatalogExportDataset> {
    let supplier_repo = SupplierRepository::new(executor);
    let contract_repo = ContractRepository::new(executor);
    let policy_repo = FiscalYearTaxPolicyRepository::new(executor);

    let suppliers = supplier_repo.list_suppliers()?;
    let unit_supplier_links = supplier_repo
        .list_all_unit_supplier_links()?
        .into_iter()
        .map(|(unit_id, supplier_id)| ContractCatalogUnitSupplierLink {
            unit_id,
            supplier_id,
        })
        .collect();
    let tax_policies = policy_repo.list_policies()?;

    let contracts = contract_repo
        .list_contracts(None, None, None)?
        .into_iter()
        .map(|contract| build_contract_row(&contract_repo, contract))
        .collect::<AppResult<Vec<_>>>()?;

    let dataset = ContractCatalogExportDataset {
        suppliers,
        unit_supplier_links,
        contracts,
        tax_policies,
    };
    validate_contract_catalog_dataset_for_export(&dataset, mode, None)?;
    Ok(dataset)
}

/// UNIT-scoped projection for one authoritative target unit code (ADR-0059).
///
/// Resolution is fail-closed through
/// [`transport_target::resolve_unit_transport_target`] (existence + exact
/// `units.code`); the internal `units.id` is then used as the relational scope
/// for all row selection — `target_node_id` (code) and relational `unit_id`
/// (id) stay distinct.
fn execute_for_unit(
    executor: DbExecutor<'_>,
    target_unit_code: &str,
) -> AppResult<ContractCatalogExportDataset> {
    let code = transport_target::resolve_unit_transport_target(executor, target_unit_code)?;
    let unit = executor.units().get_unit_by_code(&code)?.ok_or_else(|| {
        AppError::BusinessLogic(crate::errors::BusinessLogicError::OperationNotPermitted {
            message: format!("عقدة الوحدة المستهدفة غير متوفرة في السجل: {code} — رفض مغلق"),
        })
    })?;

    let supplier_repo = SupplierRepository::new(executor);
    let contract_repo = ContractRepository::new(executor);
    let policy_repo = FiscalYearTaxPolicyRepository::new(executor);

    let contracts = contract_repo
        .list_catalog_exportable_contracts_for_unit(&unit.id)?
        .into_iter()
        .map(|contract| build_contract_row_for_unit(&contract_repo, contract, &unit.id))
        .collect::<AppResult<Vec<_>>>()?;

    // UNIT↔supplier associations restricted to the target unit.
    let unit_supplier_links = supplier_repo
        .list_unit_supplier_links_for_unit(&unit.id)?
        .into_iter()
        .map(|(unit_id, supplier_id)| ContractCatalogUnitSupplierLink {
            unit_id,
            supplier_id,
        })
        .collect::<Vec<_>>();

    // Suppliers are GLOBAL / WILAYA-authoritative rows; only those REFERENCED
    // by the target unit's relationships (links + contracts) are shipped
    // (ADR-0059 §11.1). The global listing order (`name`) is preserved so the
    // referenced subset stays deterministic.
    let mut referenced: HashSet<String> = unit_supplier_links
        .iter()
        .map(|l| l.supplier_id.clone())
        .collect();
    referenced.extend(contracts.iter().map(|r| r.contract.supplier_id.clone()));
    let suppliers = supplier_repo
        .list_suppliers()?
        .into_iter()
        .filter(|s| referenced.contains(&s.id))
        .collect();

    // Tax policies remain GLOBAL regardless of the target unit (ADR-0059 §11.2).
    let tax_policies = policy_repo.list_policies()?;

    let dataset = ContractCatalogExportDataset {
        suppliers,
        unit_supplier_links,
        contracts,
        tax_policies,
    };
    validate_contract_catalog_dataset_for_export(
        &dataset,
        &ExportContractCatalogMode::UnitDistribution {
            target_unit_code: code,
        },
        Some(&unit.id),
    )?;
    Ok(dataset)
}

/// Map one contract into its dataset row with product lines / allocations /
/// exceptions (fleet: allocations via the contract chain, no unit restriction).
fn build_contract_row(
    contract_repo: &ContractRepository<'_>,
    contract: crate::models::Contract,
) -> AppResult<ContractCatalogContractRow> {
    build_contract_row_inner(contract_repo, contract, None)
}

/// Map one contract into its dataset row with product lines / allocations /
/// exceptions, restricted to one owning UNIT for the allocation scope.
fn build_contract_row_for_unit(
    contract_repo: &ContractRepository<'_>,
    contract: crate::models::Contract,
    unit_id: &str,
) -> AppResult<ContractCatalogContractRow> {
    build_contract_row_inner(contract_repo, contract, Some(unit_id))
}

fn build_contract_row_inner(
    contract_repo: &ContractRepository<'_>,
    contract: crate::models::Contract,
    unit_scope: Option<&str>,
) -> AppResult<ContractCatalogContractRow> {
    let product_lines = contract_repo
        .list_sync_contract_products(&contract.id)?
        .into_iter()
        .map(|(line, created_at)| ContractCatalogProductLine {
            id: line.id,
            contract_id: line.contract_id,
            product_id: line.product_id,
            proposed_price_ht: line.proposed_price_ht,
            agreed_price_ht: line.agreed_price_ht,
            tva_classification: line.tva_classification,
            tva_rate: line.tva_rate,
            tva_amount: line.tva_amount,
            price_ttc: line.price_ttc,
            purchase_unit: line.purchase_unit,
            consumption_unit: line.consumption_unit,
            conversion_factor: line.conversion_factor,
            created_at,
        })
        .collect();

    let mut allocations = Vec::new();
    let mut exceptions = Vec::new();
    let allocation_rows = match unit_scope {
        Some(unit_id) => {
            contract_repo.list_sync_allocations_for_contract_unit(&contract.id, unit_id)?
        }
        None => contract_repo.list_sync_allocations_for_contract(&contract.id)?,
    };
    for (allocation, created_at) in allocation_rows {
        exceptions.extend(contract_repo.list_exceptions_for_allocation(&allocation.id)?);
        allocations.push(ContractCatalogAllocationRow {
            allocation,
            created_at,
        });
    }

    Ok(ContractCatalogContractRow {
        contract,
        product_lines,
        allocations,
        exceptions,
    })
}

/// Producer-side validation of the EXACT dataset selected for the mode
/// (ADR-0059 §15.1). Mirrors the per-row structural rules of the import
/// boundary WITHOUT the import-side whole-fleet empty-catalog rejection
/// (ADR-0059 §10: a UNIT scoped artifact may legitimately carry zero
/// contracts; the fleet empty-catalog rule is an import-side policy and is
/// deliberately not duplicated here).
///
/// For `UnitDistribution`, additionally asserts every selected contract is
/// owned by the target unit — the producer selected-dataset invariant.
fn validate_contract_catalog_dataset_for_export(
    dataset: &ContractCatalogExportDataset,
    mode: &ExportContractCatalogMode,
    unit_scope_id: Option<&str>,
) -> AppResult<()> {
    for row in &dataset.contracts {
        if row.contract.id.trim().is_empty() {
            return Err(dataset_validation_error("contracts[].id", "عقد بدون معرف"));
        }
        if row.contract.contract_reference.trim().is_empty() {
            return Err(dataset_validation_error(
                "contracts[].contract_reference",
                format!("عقد «{}» بدون مرجع", row.contract.id),
            ));
        }
        if row.contract.unit_id.trim().is_empty() || row.contract.supplier_id.trim().is_empty() {
            return Err(dataset_validation_error(
                "contracts[].ownership",
                format!("عقد «{}» بدون وحدة أو مورد", row.contract.id),
            ));
        }
        if row.product_lines.is_empty() {
            return Err(dataset_validation_error(
                "contracts[].product_lines",
                format!("عقد «{}» بدون منتجات", row.contract.id),
            ));
        }
    }
    match mode {
        ExportContractCatalogMode::FleetRestore => {}
        ExportContractCatalogMode::UnitDistribution { .. } => {
            // Producer selected-dataset invariant (ADR-0059 §15.1): every
            // selected contract must belong to the target unit's internal id.
            // `unit_scope_id` is always present here (set by execute_for_unit).
            let scope = unit_scope_id.ok_or_else(|| {
                dataset_validation_error(
                    "contracts[].unit_id",
                    "نطاق الوحدة مفقود للتحقق من الحزمة الموجهة",
                )
            })?;
            for row in &dataset.contracts {
                if row.contract.unit_id != scope {
                    return Err(dataset_validation_error(
                        "contracts[].unit_id",
                        format!(
                            "عقد «{}» لا ينتمي إلى الوحدة المستهدفة — رفض مغلق",
                            row.contract.id
                        ),
                    ));
                }
            }
        }
    }
    Ok(())
}

fn dataset_validation_error(field: &str, message: impl Into<String>) -> AppError {
    AppError::Validation(ValidationError::InvalidFormat {
        field: field.into(),
        message: message.into(),
    })
}
