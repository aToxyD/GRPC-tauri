import { invoke as tauriInvoke } from '@tauri-apps/api/core';
import { open as tauriOpen, save as tauriSave, ask as tauriAsk } from '@tauri-apps/plugin-dialog';
import { getCurrentWindow as tauriGetCurrentWindow } from '@tauri-apps/api/window';
import { LogicalSize as tauriLogicalSize } from '@tauri-apps/api/dpi';
import { normalizeError } from './errors';
import { telemetry } from './telemetry';
import type {
  LoginRequest, LoginResponse, Settings, NodeConfiguration,
  User, Product, CreateProductRequest, UpdateProductRequest,
  Unit, CreateUnitRequest, InventoryStock, SupplierOrder, SupplierOrderItem, CreateOrderRequest, UpdateOrderRequest,
  MealConsumption, MealConsumptionInput, MealConsumptionItem,
  DailyReportResult, DailyReportInput, DailyConsumptionView, DailyFifoConsumptionPreview,
  StockCheckResult, MonthlySummary, DailyReport,
  MealType,
  ConsumptionItemInput, OrderItemInput,
  BackupInfo, LoginMetrics, SystemMetrics, Notification, ProgressInfo,
  SyncSecurityDiagnostics, SyncPreflightCheck,
  AuditFilters, AuditLogResponse, AuditStats, AuditEntry, SessionStatus,
  StockMovementFilters, StockMovementResponse, StockSummary, StockMovement,
  ComputeSnapshotResult, UnitInventoryView, XlsxExportResult,
  SyncExportResult, SyncImportResult, DailyReportImportResult, UnitNodePackageImportResult, StockMovementsImportResult,
  AuditChainStatus, AuditHealthReport, SystemHealthReport, SyncNodeHealth,
  ConflictSummary, SyncConflict,
  FiscalClosurePreview, FiscalClosureApplyResult, FiscalYearStatus,
  FiscalTransitionHistoryEntry, FiscalPackageRegistryEntry,
  BuildInfo, TelemetryEvent,
  InventoryStockPageView,
} from './types';

export async function safeInvoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const startTime = performance.now();
  try {
    const result = await tauriInvoke<T>(cmd, args);
    const duration = performance.now() - startTime;
    telemetry.trackLatency(cmd, duration);
    telemetry.trackOperation(cmd, duration);
    return result;
  } catch (error) {
    const duration = performance.now() - startTime;
    const normalized = normalizeError(error);
    telemetry.trackLatency(cmd, duration);
    telemetry.trackError(error, `IPC Command: ${cmd}`);
    telemetry.trackAsyncFailure(cmd, normalized.message);
    console.error(`[IPC Error] Command "${cmd}" failed:`, normalized.originalError);
    throw new Error(normalized.message);
  }
}


// Authentication
export async function login(request: LoginRequest): Promise<LoginResponse> {
  return await safeInvoke('login', { request });
}

export async function changePassword(userId: string, newPassword: string): Promise<boolean> {
  return await safeInvoke('change_password', { target_user_id: userId, new_password: newPassword });
}

// Session Management
export async function logout(): Promise<boolean> {
  return await safeInvoke('logout');
}

export async function checkSession(): Promise<SessionStatus> {
  return await safeInvoke('check_session');
}

export async function getCurrentUser(): Promise<User | null> {
  return await safeInvoke<User | null>('get_current_user');
}

export async function touchSession(): Promise<void> {
  return await safeInvoke('touch_session');
}


// Configuration
export async function getSettings(): Promise<Settings> {
  return await safeInvoke('get_settings');
}

export async function configureAsWilaya(wilayaCode: string, wilayaName: string): Promise<Settings> {
  return await safeInvoke('configure_as_wilaya', { wilayaCode, wilayaName });
}

export async function isConfigured(): Promise<boolean> {
  return await safeInvoke('is_configured');
}

// Products
export async function createProduct(request: CreateProductRequest): Promise<string> {
  return await safeInvoke('create_product', { request });
}

export async function updateProduct(request: UpdateProductRequest): Promise<void> {
  return await safeInvoke('update_product', { request });
}

export async function deleteProduct(productId: string): Promise<void> {
  return await safeInvoke('delete_product', { productId });
}

export async function getProduct(productId: string): Promise<Product | null> {
  return await safeInvoke('get_product', { productId });
}

export async function listProducts(): Promise<Product[]> {
  return await safeInvoke('list_products');
}

// Units
export async function createUnit(request: CreateUnitRequest, wilayaCode: string): Promise<Unit> {
  return await safeInvoke('create_unit', { request, wilayaCode });
}

export async function getUnit(unitId: string): Promise<Unit | null> {
  return await safeInvoke('get_unit', { unitId });
}

export async function listUnits(wilayaCode: string): Promise<Unit[]> {
  return await safeInvoke('list_units', { wilayaCode });
}

export async function updateUnit(unitId: string, request: CreateUnitRequest): Promise<void> {
  return await safeInvoke('update_unit', { unitId, request });
}

export async function deleteUnit(unitId: string): Promise<void> {
  return await safeInvoke('delete_unit', { unitId });
}

// Stock
export async function getStock(productId: string): Promise<InventoryStock | null> {
  return await safeInvoke('get_stock', { productId });
}

export async function getAllStocks(): Promise<InventoryStock[]> {
  return await safeInvoke('get_all_stocks');
}

export async function checkStockAvailability(items: ConsumptionItemInput[]): Promise<StockCheckResult[]> {
  return await safeInvoke('check_stock_availability', { items });
}

// Orders
export async function createSupplierOrder(request: CreateOrderRequest): Promise<{ orderId: string; totalAmount: number }> {
  const [orderId, totalAmount] = await safeInvoke<[string, number]>('create_supplier_order', { request });
  return { orderId, totalAmount };
}

export async function confirmOrder(orderId: string): Promise<void> {
  return await safeInvoke('confirm_order', { orderId });
}

export async function updateSupplierOrder(request: UpdateOrderRequest): Promise<number> {
  return await safeInvoke('update_supplier_order', { request });
}

export async function deleteSupplierOrder(orderId: string): Promise<void> {
  return await safeInvoke('delete_supplier_order', { orderId });
}

export async function getSupplierOrder(orderId: string): Promise<SupplierOrder | null> {
  return await safeInvoke('get_supplier_order', { orderId });
}

export async function getSupplierOrderItems(orderId: string): Promise<SupplierOrderItem[]> {
  return await safeInvoke('get_supplier_order_items', { orderId });
}

export async function listSupplierOrders(): Promise<SupplierOrder[]> {
  return await safeInvoke('list_supplier_orders');
}

export async function createOrder(request: CreateOrderRequest): Promise<{ orderId: string; totalAmount: number }> {
  const [orderId, totalAmount] = await safeInvoke<[string, number]>('create_order', { request });
  return { orderId, totalAmount };
}

// Daily Reports
export async function createDailyReport(input: DailyReportInput, unitId?: string): Promise<DailyReportResult> {
  return await safeInvoke('create_daily_report', { input, unitId });
}

/** FIFO dry-run preview — same engine as daily report execution. */
export async function previewDailyConsumptionFifo(
  input: DailyReportInput
): Promise<DailyFifoConsumptionPreview> {
  return await safeInvoke('preview_daily_consumption_fifo', { input });
}

/** @deprecated Use createDailyReport */
export async function createMealConsumption(input: DailyReportInput, unitId?: string): Promise<DailyReportResult> {
  return createDailyReport(input, unitId);
}

export async function getDailyReport(reportId: string): Promise<DailyReportResult> {
  return await safeInvoke('get_daily_report', { reportId });
}

export async function listDailyReports(startDate?: string, endDate?: string, fiscalYear?: number, month?: number): Promise<DailyReport[]> {
  return await safeInvoke('list_daily_reports', { startDate, endDate, fiscalYear, month });
}

export async function listFiscalYears(): Promise<number[]> {
  return await safeInvoke('list_fiscal_years');
}

export type ReportType = 'daily' | 'monthly' | 'stock';

export interface WilayaReportList {
  type: 'Daily' | 'Monthly' | 'Stock';
  data: DailyReport[] | MonthlySummary[] | StockMovement[];
}

export async function listWilayaReports(
  unitId: string | null,
  reportType: ReportType,
  year?: number,
  month?: number
): Promise<WilayaReportList> {
  return await safeInvoke('list_wilaya_reports', { unitId, reportType, year, month });
}

// Calculations - All computed in Rust
export async function calculateMealCost(items: [number, number][]): Promise<number> {
  return await safeInvoke('calculate_meal_cost', { items });
}

export async function calculateMealRate(
  totalCost: number,
  staff24hCount: number,
  staff8hCount: number,
  reservationCount: number,
  missionCount: number,
  guestCount: number
): Promise<number> {
  return await safeInvoke('calculate_meal_rate', {
    totalCost,
    staff_24h_count: staff24hCount,
    staff_8h_count: staff8hCount,
    reservation_count: reservationCount,
    mission_count: missionCount,
    guest_count: guestCount,
  });
}

export async function calculateProductPriceWithTva(basePrice: number, tva: number): Promise<number> {
  return await safeInvoke('calculate_product_price_with_tva', { basePrice, tva });
}

export async function getMonthlySummary(year: number, month?: number): Promise<MonthlySummary> {
  return await safeInvoke('get_monthly_summary', { year, month });
}

// Sync package export
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


// Excel Export
export async function exportProductsExcel(filePath: string): Promise<XlsxExportResult> {
  return await safeInvoke('export_products_excel', { filePath });
}

export async function exportDailyReportExcel(reportId: string, filePath: string): Promise<XlsxExportResult> {
  return await safeInvoke('export_daily_report_excel', { reportId, filePath });
}

export async function exportMonthlySummaryExcel(year: number, month: number, filePath: string): Promise<XlsxExportResult> {
  return await safeInvoke('export_monthly_summary_excel', { year, month, filePath });
}


// Sync package import (SECURE with internal decryption & integrity check)
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


// Additional functions for unit operations
export async function getCurrentStock(): Promise<InventoryStock[]> {
  return await safeInvoke('get_current_stock');
}

export async function getDailyConsumption(date: string): Promise<DailyConsumptionView | null> {
  return await safeInvoke('get_daily_consumption', { date });
}

export async function getOrders(): Promise<SupplierOrder[]> {
  return await safeInvoke('get_orders');
}

export async function recordConsumption(consumption: MealConsumptionInput): Promise<void> {
  return await safeInvoke('record_consumption', { consumption });
}

export async function generateReports(type: string, month: number, year: number): Promise<void> {
  return await safeInvoke('generate_reports', { type, month, year });
}

export async function getReportData<T = unknown>(type: string, month: number, year: number): Promise<T> {
  return await safeInvoke('get_report_data', { type, month, year });
}

// Backup Management
export async function createBackup(): Promise<string> {
  return await safeInvoke('create_backup');
}

export async function listBackups(): Promise<BackupInfo[]> {
  return await safeInvoke('list_backups');
}

export async function issueOperationExecutionToken(request: {
  operation: 'fiscal_close' | 'archive' | 'restore' | 'import_historical';
  year?: number;
  next_year?: number;
}): Promise<{ token: string; operation: string }> {
  return await safeInvoke('issue_operation_execution_token', { request });
}

export async function restoreBackup(
  backupPath: string,
  confirmation: string,
  executionToken: string,
): Promise<void> {
  return await safeInvoke('restore_backup', {
    backupPath,
    confirmation,
    executionToken,
  });
}

// Metrics and Monitoring
export async function getLoginMetrics(): Promise<LoginMetrics> {
  return await safeInvoke('get_login_metrics');
}

export async function getSystemMetrics(): Promise<SystemMetrics> {
  return await safeInvoke('get_system_metrics');
}

export async function getSyncSecurityDiagnostics(): Promise<SyncSecurityDiagnostics> {
  return await safeInvoke('get_sync_security_diagnostics');
}

export async function syncPreflightCheck(): Promise<SyncPreflightCheck> {
  return await safeInvoke('sync_preflight_check');
}

// Audit Trail
export async function getAuditLog(
  filters: AuditFilters,
  page: number = 0,
  pageSize: number = 50
): Promise<AuditLogResponse> {
  return await safeInvoke('get_audit_log', { filters, page, pageSize });
}

export async function getAuditStats(
  startDate: string,
  endDate: string
): Promise<AuditStats> {
  return await safeInvoke('get_audit_stats', { startDate, endDate });
}

export async function getUserActivity(
  userId: string,
  days: number = 30
): Promise<AuditEntry[]> {
  return await safeInvoke('get_user_activity', { userId, days });
}

export async function exportAuditLogExcel(
  filters: AuditFilters,
  filePath: string
): Promise<XlsxExportResult> {
  return await safeInvoke('export_audit_log_excel', { filters, filePath });
}

export async function cleanupAuditLogs(beforeDate?: string): Promise<number> {
  return await safeInvoke('cleanup_audit_logs', { beforeDate });
}

// Stock Movement Ledger
export async function getStockMovements(
  filters: StockMovementFilters = {},
  page: number = 0,
  pageSize: number = 50,
): Promise<StockMovementResponse> {
  return await safeInvoke('get_stock_movements', { filters, page, pageSize });
}

export async function getStockSummary(): Promise<StockSummary[]> {
  return await safeInvoke('get_stock_summary');
}


// Unit & Wilaya
export async function exportAllUnitsMonthlyStatusExcel(
  year: number,
  month: number,
  filePath: string,
): Promise<{ success: boolean; count: number; filePath: string }> {
  return await safeInvoke('export_all_units_monthly_status_excel', { year, month, filePath });
}

export async function exportStockMovementsExcel(
  productId: string | undefined,
  filePath: string,
): Promise<XlsxExportResult> {
  return await safeInvoke('export_stock_movements_excel', {
    productId, filePath
  });
}

// FIFO Inventory View (Phase 2)
export async function getInventoryFifoView(): Promise<InventoryStockPageView> {
  return await safeInvoke('get_inventory_fifo_view');
}

// Unit Monthly Inventory Snapshots

export async function computeUnitInventorySnapshot(
  unitId: string,
  year: number,
  month: number,
  forceRecompute = false,
): Promise<ComputeSnapshotResult> {
  return await safeInvoke('compute_unit_inventory_snapshot',
    { unitId, year, month, forceRecompute });
}

export async function getUnitInventoryView(
  unitId: string,
  year: number,
  month: number,
): Promise<UnitInventoryView | null> {
  return await safeInvoke('get_unit_inventory_view', { unitId, year, month });
}

export async function getAvailableReportMonths(
  unitId: string,
): Promise<[number, number][]> {
  return await safeInvoke('get_available_report_months', { unitId });
}

export async function exportUnitInventoryExcel(
  unitId: string,
  year: number,
  month: number,
  filePath: string,
): Promise<XlsxExportResult> {
  return await safeInvoke('export_unit_inventory_excel',
    { unitId, year, month, filePath });
}

// ─── Observability ────────────────────────────────────────────────────────────

export async function getAuditChainStatus(): Promise<AuditChainStatus> {
  return await safeInvoke('get_audit_chain_status');
}

export async function getAuditHealth(): Promise<AuditHealthReport> {
  return await safeInvoke('get_audit_health');
}

export async function getSystemHealth(): Promise<SystemHealthReport> {
  return await safeInvoke('get_system_health');
}

export async function getSyncHealth(): Promise<SyncNodeHealth[]> {
  return await safeInvoke('get_sync_health');
}

export async function getConflictSummary(): Promise<ConflictSummary> {
  return await safeInvoke('get_conflict_summary');
}

export async function listSyncConflicts(unresolvedOnly: boolean = false): Promise<SyncConflict[]> {
  return await safeInvoke('list_sync_conflicts', { unresolvedOnly });
}

export async function resolveSyncConflict(conflictId: string, note: string): Promise<void> {
  return await safeInvoke('resolve_sync_conflict', { conflictId, note });
}

// ── Fiscal Lifecycle ─────────────────────────────────────────────────────────

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

export async function getBuildInfo(): Promise<BuildInfo> {
  return await safeInvoke('get_build_info');
}

export async function getRecentTelemetry(limit: number): Promise<TelemetryEvent[]> {
  return await safeInvoke('get_recent_telemetry', { limit });
}

// Platform API Wrappers
export async function openFile(options?: Parameters<typeof tauriOpen>[0]) {
  return await tauriOpen(options);
}

export async function saveFile(options?: Parameters<typeof tauriSave>[0]) {
  return await tauriSave(options);
}

export async function showAsk(message: string, options?: Parameters<typeof tauriAsk>[1]): Promise<boolean> {
  return await tauriAsk(message, options);
}

export function getAppWindow() {
  return tauriGetCurrentWindow();
}

export function createLogicalSize(width: number, height: number) {
  return new tauriLogicalSize(width, height);
}

export async function listenToResize(callback: () => void): Promise<() => void> {
  const win = tauriGetCurrentWindow();
  const unlisten = await win.onResized(() => {
    callback();
  });
  return unlisten;
}



