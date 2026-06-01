import { safeInvoke } from '../tauri';
import type {
  FiscalClosurePreview, FiscalClosureApplyResult, FiscalYearStatus,
  FiscalTransitionHistoryEntry, FiscalPackageRegistryEntry,
} from '../types';

export interface CloseFiscalYearRequest {
  year: number;
  next_year: number;
}

export interface CloseFiscalYearResponse {
  closed_year: number;
  opened_year: number;
  snapshot_count: number;
}

export async function closeFiscalYear(
  request: CloseFiscalYearRequest,
): Promise<CloseFiscalYearResponse> {
  return await safeInvoke('close_fiscal_year', { request });
}

export async function getFiscalYearStatus(
  year: number,
): Promise<FiscalYearStatus | null> {
  return await safeInvoke('get_fiscal_year_status', { year });
}

export async function exportFiscalClosurePackage(
  closedYear: number,
  openedYear: number,
  closureTimestampUtc: string,
  filePath: string,
  transitionId: string | null = null,
): Promise<string> {
  return await safeInvoke('export_fiscal_closure_package', {
    closedYear,
    openedYear,
    closureTimestampUtc,
    filePath,
    transitionId,
  });
}

export async function previewFiscalClosurePackage(
  filePath: string,
): Promise<FiscalClosurePreview> {
  return await safeInvoke('preview_fiscal_closure_package', { filePath });
}

export async function applyFiscalClosurePackage(
  filePath: string,
  confirmation: string,
): Promise<FiscalClosureApplyResult> {
  return await safeInvoke('apply_fiscal_closure_package', { filePath, confirmation });
}

export async function getFiscalTransitionHistory(): Promise<FiscalTransitionHistoryEntry[]> {
  return await safeInvoke('get_fiscal_transition_history');
}

export async function listFiscalPackageRegistry(): Promise<FiscalPackageRegistryEntry[]> {
  return await safeInvoke('list_fiscal_package_registry');
}

export async function updateFiscalPackageRetentionStatus(
  transitionId: string,
  status: 'ACTIVE' | 'ARCHIVED' | 'RETIRED',
  confirmation: string,
): Promise<void> {
  return await safeInvoke('update_fiscal_package_retention_status', {
    transition_id: transitionId,
    status,
    confirmation,
  });
}

export async function getAdvancedDiagnosticsBundle(fiscalYear: number | null): Promise<unknown> {
  return await safeInvoke('get_advanced_diagnostics_bundle', { fiscalYear });
}

export async function verifyInventoryIntegrity(year: number): Promise<unknown> {
  return await safeInvoke('verify_inventory_integrity', { year });
}

export async function createFiscalOperationalSnapshot(fiscalYear: number): Promise<void> {
  return await safeInvoke('create_fiscal_operational_snapshot', { fiscalYear });
}
