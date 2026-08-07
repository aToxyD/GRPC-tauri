import { safeInvoke } from '../tauri';
import type {
  SyncExportResult, SyncImportResult, DailyReportImportResult,
  UnitNodePackageImportResult, StockMovementsImportResult,
  IdentityAccessImportResult,
} from '../types';

export async function exportProductsPackage(filePath: string): Promise<SyncExportResult> {
  return await safeInvoke('export_products_package', { filePath });
}

export async function exportDailyReportPackage(reportId: string, filePath: string): Promise<SyncExportResult> {
  return await safeInvoke('export_daily_report_package', { reportId, filePath });
}

export async function exportMonthlySummaryPackage(year: number, month: number, filePath: string): Promise<SyncExportResult> {
  return await safeInvoke('export_monthly_summary_package', { year, month, filePath });
}

export async function exportUnitNodePackage(unitId: string, filePath: string): Promise<SyncExportResult> {
  return await safeInvoke('export_unit_node_package', { unitId, filePath });
}

export async function exportStockMovementsPackage(startDate: string, endDate: string, filePath: string): Promise<SyncExportResult> {
  return await safeInvoke('export_stock_movements_package', { startDate, endDate, filePath });
}

export async function importProductsPackage(filePath: string): Promise<SyncImportResult> {
  return await safeInvoke('import_products_package', { filePath });
}

export async function importDailyReportPackage(filePath: string, unitId: string): Promise<DailyReportImportResult> {
  return await safeInvoke('import_daily_report_package', { filePath, unitId });
}

export async function importUnitNodePackage(filePath: string): Promise<UnitNodePackageImportResult> {
  return await safeInvoke('import_unit_node_package', { filePath });
}

export async function importMonthlySummaryPackage(filePath: string, unitId: string): Promise<DailyReportImportResult> {
  return await safeInvoke('import_monthly_summary_package', { filePath, unitId });
}

export async function importStockMovementsPackage(filePath: string, unitId: string): Promise<StockMovementsImportResult> {
  return await safeInvoke('import_stock_movements_package', { filePath, unitId });
}

export async function setFleetAdminPassword(password: string): Promise<void> {
  return await safeInvoke('set_fleet_admin_password', { password });
}

export async function setUnitUserPassword(unitCode: string, password: string): Promise<void> {
  return await safeInvoke('set_unit_user_password', { unitCode, password });
}

export async function setAccountStatus(username: string, enabled: boolean): Promise<void> {
  return await safeInvoke('set_account_status', { username, enabled });
}

export async function exportIdentityAccessPackage(unitCode: string, filePath: string): Promise<SyncExportResult> {
  return await safeInvoke('export_identity_access_package', { unitCode, filePath });
}

export async function importIdentityAccessPackage(filePath: string): Promise<IdentityAccessImportResult> {
  return await safeInvoke('import_identity_access_package', { filePath });
}
