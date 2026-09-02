import { safeInvoke } from '../tauri';
import type {
  AddContractProductRequest,
  AssociateUnitSupplierRequest,
  Contract,
  ContractAllocation,
  ContractAllocationException,
  ContractProduct,
  ContractStatus,
  ContractTransitionRequest,
  CreateContractRequest,
  CreateSupplierRequest,
  FiscalYearTaxPolicy,
  ReleaseContractAllocationRequest,
  RevokeContractAllocationReleaseRequest,
  SetAgreedPriceRequest,
  SetSupplierActiveRequest,
  SetTaxPolicyRequest,
  Supplier,
  UpdateSupplierRequest,
} from '../types';

// Suppliers (WILAYA admin only)
export async function createSupplier(
  request: CreateSupplierRequest,
): Promise<Supplier> {
  return await safeInvoke('create_supplier', { request });
}

export async function updateSupplier(
  request: UpdateSupplierRequest,
): Promise<Supplier> {
  return await safeInvoke('update_supplier', { request });
}

export async function setSupplierActive(
  request: SetSupplierActiveRequest,
): Promise<Supplier> {
  return await safeInvoke('set_supplier_active', { request });
}

export async function associateSupplierWithUnit(
  request: AssociateUnitSupplierRequest,
): Promise<void> {
  return await safeInvoke('associate_supplier_with_unit', { request });
}

export async function disassociateSupplierFromUnit(
  request: AssociateUnitSupplierRequest,
): Promise<void> {
  return await safeInvoke('disassociate_supplier_from_unit', { request });
}

// Contracts (WILAYA admin only)
export async function createContract(
  request: CreateContractRequest,
): Promise<Contract> {
  return await safeInvoke('create_contract', { request });
}

export async function addContractProduct(
  request: AddContractProductRequest,
): Promise<[string, string]> {
  return await safeInvoke('add_contract_product', { request });
}

export async function setContractProductAgreedPrice(
  request: SetAgreedPriceRequest,
): Promise<number> {
  return await safeInvoke('set_contract_product_agreed_price', { request });
}

export async function acceptContract(
  request: ContractTransitionRequest,
): Promise<Contract> {
  return await safeInvoke('accept_contract', { request });
}

export async function activateContract(
  request: ContractTransitionRequest,
): Promise<Contract> {
  return await safeInvoke('activate_contract', { request });
}

export async function endContract(
  request: ContractTransitionRequest,
): Promise<Contract> {
  return await safeInvoke('end_contract', { request });
}

export async function cancelContract(
  request: ContractTransitionRequest,
): Promise<Contract> {
  return await safeInvoke('cancel_contract', { request });
}

export async function releaseContractAllocation(
  request: ReleaseContractAllocationRequest,
): Promise<ContractAllocationException> {
  return await safeInvoke('release_contract_allocation', { request });
}

export async function revokeContractAllocationRelease(
  request: RevokeContractAllocationReleaseRequest,
): Promise<void> {
  return await safeInvoke('revoke_contract_allocation_release', { request });
}

// Fiscal-year TVA policy (WILAYA admin only)
export async function setFiscalTaxPolicy(
  request: SetTaxPolicyRequest,
): Promise<FiscalYearTaxPolicy> {
  return await safeInvoke('set_fiscal_tax_policy', { request });
}

export async function getFiscalTaxPolicy(
  fiscalYear: number,
): Promise<FiscalYearTaxPolicy | null> {
  return await safeInvoke('get_fiscal_tax_policy', { fiscalYear });
}

export async function listFiscalTaxPolicies(): Promise<FiscalYearTaxPolicy[]> {
  return await safeInvoke('list_fiscal_tax_policies');
}

// Read projections (WILAYA admin)
export async function getSupplier(
  supplierId: string,
): Promise<Supplier | null> {
  return await safeInvoke('get_supplier', { supplierId });
}

export async function listSuppliers(): Promise<Supplier[]> {
  return await safeInvoke('list_suppliers');
}

export async function listUnitSuppliers(
  unitId: string,
): Promise<Supplier[]> {
  return await safeInvoke('list_unit_suppliers', { unitId });
}

export async function getContract(
  contractId: string,
): Promise<Contract | null> {
  return await safeInvoke('get_contract', { contractId });
}

export async function listContracts(
  unitId?: string | null,
  supplierId?: string | null,
  fiscalYear?: number | null,
): Promise<Contract[]> {
  return await safeInvoke('list_contracts', {
    unitId: unitId ?? null,
    supplierId: supplierId ?? null,
    fiscalYear: fiscalYear ?? null,
  });
}

export async function getContractProducts(
  contractId: string,
): Promise<ContractProduct[]> {
  return await safeInvoke('get_contract_products', { contractId });
}

export async function listContractAllocations(
  contractId: string,
): Promise<ContractAllocation[]> {
  return await safeInvoke('list_contract_allocations', { contractId });
}

export async function listAllocationExceptions(
  allocationId: string,
): Promise<ContractAllocationException[]> {
  return await safeInvoke('list_allocation_exceptions', { allocationId });
}

export type { ContractStatus };