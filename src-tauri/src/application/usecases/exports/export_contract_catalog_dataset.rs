//! Build the WILAYA-authoritative ContractCatalog dataset (ADR-0055 /
//! SEC-087-F sync ContractCatalog V2, WILAYA → UNIT).
//!
//! The payload is a read-only projection: suppliers, UNIT↔supplier
//! associations, contracts (headers + product lines + per-UNIT allocations +
//! allocation release exceptions), and the fiscal-year TVA policies. UNIT
//! importers apply ONLY the rows scoped to their own unit id.

use crate::application::usecases::exports::types::{
    ContractCatalogAllocationRow, ContractCatalogContractRow, ContractCatalogExportDataset,
    ContractCatalogProductLine, ContractCatalogUnitSupplierLink, ExportContractCatalogInput,
};
use crate::errors::AppResult;
use crate::repositories::{
    ContractRepository, DbExecutor, FiscalYearTaxPolicyRepository, SupplierRepository,
};

pub fn execute(
    executor: DbExecutor<'_>,
    _input: ExportContractCatalogInput,
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
        .map(|contract| {
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
            for (allocation, created_at) in
                contract_repo.list_sync_allocations_for_contract(&contract.id)?
            {
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
        })
        .collect::<AppResult<Vec<_>>>()?;

    Ok(ContractCatalogExportDataset {
        suppliers,
        unit_supplier_links,
        contracts,
        tax_policies,
    })
}
