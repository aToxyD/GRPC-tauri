// Authentication Types
export interface LoginRequest {
  username: string;
  password: string;
}

export interface LoginResponse {
  success: boolean;
  user: User | null;
  message: string;
  requires_configuration: boolean;
  /** B6-A (ADR-0038): true when the node has an ACTIVE ADMIN identity, making the admin-key Challenge–Response the mandatory login path. */
  identity_challenge_required: boolean;
}

export interface User {
  id: string;
  username: string;
  role: 'Admin' | 'User';
  created_at: string;
}

// Configuration Types
export interface Settings {
  id: number;
  node_type: 'WILAYA' | 'UNIT';
  unit_name: string | null;
  unit_code: string | null;
  current_year: number;
  wilaya_code: string | null;
  wilaya_name: string | null;
  configured: boolean;
}

export interface NodeConfiguration {
  node_type: 'WILAYA' | 'UNIT';
  unit_name: string | null;
  wilaya_code: string | null;
}

// Product Types
export interface Product {
  id: string;
  name: string;
  base_price: number;
  tva: number;
  supplier_name: string | null;
  year: number;
  created_at: string;
}

export interface CreateProductRequest {
  name: string;
  base_price: number;
  tva: number;
  supplier_name: string | null;
}

export interface UpdateProductRequest {
  id: string;
  name: string;
  base_price: number;
  tva: number;
  supplier_name: string | null;
}

// Unit Types
export interface Unit {
  id: string;
  code: string;
  name: string;
  wilaya_code: string;
  user_id: string | null;
  created_at: string;
}

export interface CreateUnitRequest {
  code: string;
  name: string;
  username: string;
  password: string;
}

// Stock Types
export interface InventoryStock {
  id: string;
  product_id: string;
  product_name: string;
  quantity: number;
  unit: string;
  last_updated: string;
}

export interface StockCheckResult {
  available: boolean;
  product_id: string;
  product_name: string;
  requested: number;
  available_stock: number;
  deficit: number;
}

// Order Types
export interface SupplierOrder {
  id: string;
  order_date: string;
  supplier_name: string;
  reference_number: string | null;
  total_amount: number | null;
  status: 'Draft' | 'Confirmed' | 'Received' | 'Cancelled';
  created_at: string;
}

export interface SupplierOrderItem {
  id: string;
  order_id: string;
  product_id: string;
  product_name: string;
  quantity: number;
  unit_price: number;
  total_cost: number;
}

export interface CreateOrderRequest {
  supplier_name: string;
  reference_number: string | null;
  items: OrderItemInput[];
}

export interface UpdateOrderRequest {
  id: string;
  supplier_name: string;
  reference_number: string | null;
  items: OrderItemInput[];
}

export interface OrderItemInput {
  product_id: string;
  quantity: number;
  unit_price: number;
}

// Daily consumption report (one per date + unit, three meal sections)
export type MealType = 'breakfast' | 'lunch' | 'dinner';

export interface DailyReport {
  id: string;
  date: string;
  unit_id: string | null;
  total_daily_cost: number;
  total_daily_average: number;
  total_daily_beneficiaries: number;
  created_at: string;
  fiscal_year: number;
}

export interface DailyReportMeal {
  id: string;
  daily_report_id: string;
  meal_type: MealType;
  staff_24h_count: number;
  staff_8h_count: number;
  reservation_count: number;
  mission_count: number;
  guest_count: number;
  total_beneficiaries: number;
  total_meal_cost: number;
  meal_average: number;
}

export interface DailyReportMealItem {
  id: string;
  meal_id: string;
  product_id: string;
  product_name: string;
  quantity: number;
  unit_price: number;
  total_cost: number;
}

export interface MealSectionResult {
  meal: DailyReportMeal;
  items: DailyReportMealItem[];
}

export interface MealSectionInput {
  meal_type: MealType;
  staff_24h_count: number;
  staff_8h_count: number;
  reservation_count: number;
  mission_count: number;
  guest_count: number;
  items: ConsumptionItemInput[];
}

export interface DailyReportInput {
  date: string;
  meals: MealSectionInput[];
}

export interface DailyReportResult {
  report: DailyReport;
  meals: MealSectionResult[];
  daily_summary: DailyConsumptionSummary;
}

export interface DailyConsumptionSummary {
  breakfast_beneficiaries: number;
  lunch_beneficiaries: number;
  dinner_beneficiaries: number;
  breakfast_cost: number;
  lunch_cost: number;
  dinner_cost: number;
  breakfast_average: number;
  lunch_average: number;
  dinner_average: number;
  total_daily_beneficiaries: number;
  total_daily_cost: number;
  daily_average: number;
}

export type DailyConsumptionView = DailyReportResult;

// Legacy aliases
export type MealConsumption = DailyReportMeal;
export type MealConsumptionItem = DailyReportMealItem;
export type MealConsumptionResult = MealSectionResult;
export type MealConsumptionInput = MealSectionInput;
export type DailyConsumptionInput = DailyReportInput;
export type DailyConsumptionItem = DailyReportMealItem;

export interface ConsumptionItemInput {
  product_id: string;
  quantity: number;
}

export interface ConsumedLayerPortion {
  layer_id: string;
  quantity: number;
  unit_cost: number;
  total_cost: number;
}

export interface ProductFifoPreview {
  product_id: string;
  quantity: number;
  predicted_fifo_cost: number;
  unit_cost: number;
  predicted_consumption_layers: ConsumedLayerPortion[];
}

export interface MealFifoPreview {
  meal_type: string;
  predicted_fifo_cost: number;
  total_beneficiaries: number;
  meal_average: number;
  product_previews: ProductFifoPreview[];
}

export interface DailyFifoConsumptionPreview {
  predicted_fifo_cost: number;
  predicted_consumption_layers: ProductFifoPreview[];
  predicted_remaining_inventory_value: number;
  meal_previews: MealFifoPreview[];
  daily_summary: DailyConsumptionSummary;
}

// Summary Types
export interface MonthlySummary {
  month: number;
  year: number;
  total_beneficiaries: number;
  total_consumption_value: number;
  breakfast_average: number;
  lunch_average: number;
  dinner_average: number;
  daily_average: number;
  report_count: number;
}

// CSV Types
export interface SyncExportResult {
  file_path: string;
  record_count: number;
  success: boolean;
  message: string;
  file_hash: string;
}

export interface XlsxExportResult {
  file_path: string;
  record_count: number;
  success: boolean;
  message: string;
}

// Node Package Types
export interface UnitNodePackage {
  unit: Unit;
  user: UserExport;
}

export interface UserExport {
  username: string;
  password_hash: string;
  role: string;
}

// Additional types for unit operations
export interface StockItem {
  id: string;
  product_id: string;
  product_name: string;
  quantity: number;
  unit: string;
  last_updated: string;
}

export interface ConsumptionRecord {
  id: string;
  product_name: string;
  quantity: number;
  meal_type: string;
  consumption_time: string;
  date: string;
}

export interface Order {
  id: string;
  created_at: string;
  status: string;
  total: number | null;
}

// Backup Types
export interface BackupInfo {
  filename: string;
  path: string;
  size_bytes: number;
  created: string;
}

// Metrics and Monitoring Types
export interface LoginMetrics {
  total_attempts: number;
  failed_attempts: number;
  current_lockout: boolean;
  lockout_reason: string | null;
  lockout_time_remaining: number | null;
}

export interface SystemMetrics {
  database_size: number;
  backup_count: number;
  uptime: number;
  memory_usage: number;
  total_products: number;
  units_count: number;
  cache_hit_rate: number;
  last_backup: string | null;
  daily_reports: number;
  monthly_reports: number;
  today_orders: number;
  active_users: number;
}

// SEC-007 (ADR-0047) / SEC-008 (ADR-0048): V1/HMAC diagnostics fields and the
// fiscal closure HMAC key fields were removed — package signing uses node
// identities (Ed25519), never shared env secrets.
export interface SyncSecurityDiagnostics {
  production_mode: boolean;
  has_app_key_env: boolean;
  bootstrap_would_fail: boolean;
}

export interface SyncPreflightCheck {
  status: 'ok' | 'warn' | 'fail' | string;
  reasons: string[];
  reason_messages_ar: string[];
  diagnostics: SyncSecurityDiagnostics;
}

// Notification Types
export interface Notification {
  id: string;
  type: 'success' | 'error' | 'warning' | 'info' | 'progress';
  title: string;
  message: string;
  timestamp: string;
  auto_close?: boolean;
  progress_value?: number;
}

// Sync/CSV import result counts
export interface SyncImportResult {
  added: number;
  updated: number;
  deleted: number;
}

export interface DailyReportImportResult {
  report_count: number;
  item_count: number;
  unit_id: string | null;
  file_hash: string;
  imported_by: string;
  timestamp: string;
}

export interface UnitNodePackageImportResult {
  unit_id: string;
  unit_code: string;
  unit_name: string;
  username: string;
  file_hash: string;
  imported_by: string;
  timestamp: string;
}

export interface StockMovementsImportResult {
  movement_count: number;
  unit_id: string;
  file_hash: string;
  imported_by: string;
  timestamp: string;
}

export interface IdentityAccessImportResult {
  admin_updated: boolean;
  user_updated: boolean;
  user_renamed: boolean;
  package_id: string;
  imported_by: string;
  timestamp: string;
}

// Admin-Only account synchronization result (admin_access, ADR-0051).
// Carries synchronization status ONLY — never credential material; the UNIT
// operator account is structurally out of scope for this kind.
export interface AdminAccessImportResult {
  admin_updated: boolean;
  package_id: string;
  imported_by: string;
  timestamp: string;
}

// Progress Types
export interface ProgressInfo {
  id: string;
  operation: string;
  current: number;
  total: number;
  message: string;
  status: 'running' | 'completed' | 'error';
}

// Audit Trail Types
export interface AuditEntry {
  id: string;
  user_id: string;
  username: string;
  action: string;
  action_display: string;
  entity_type: string;
  entity_type_display: string;
  entity_id: string | null;
  entity_name: string | null;
  old_value: Record<string, unknown> | null;
  new_value: Record<string, unknown> | null;
  session_id: string | null;
  timestamp: string;
  status: 'Success' | 'Failed';
  error_message: string | null;
  metadata: Record<string, unknown> | null;
}

export interface AuditFilters {
  user_id?: string;
  action?: string;
  entity_type?: string;
  start_date?: string;
  end_date?: string;
  status?: string;
  search?: string;
}

export interface AuditLogResponse {
  entries: AuditEntry[];
  total_count: number;
  page: number;
  page_size: number;
  has_more: boolean;
}

export interface AuditStats {
  total_operations: number;
  failed_operations: number;
  success_rate: number;
  most_active_users: UserActivitySummary[];
  operations_by_type: OperationCount[];
  operations_by_day: DailyOperationCount[];
  period_start: string;
  period_end: string;
}

export interface UserActivitySummary {
  user_id: string;
  username: string;
  total_operations: number;
  failed_operations: number;
  last_activity: string;
}

export interface OperationCount {
  action: string;
  action_display: string;
  count: number;
}

export interface DailyOperationCount {
  date: string;
  total: number;
  failed: number;
}

// Session Management Types
export interface SessionStatus {
  is_active: boolean;
  is_expired: boolean;
  should_warn: boolean;
  remaining_minutes: number;
  username: string | null;
}

// Stock Movement Ledger Types
export type StockMovementType = 'IN' | 'OUT' | 'OPENING';

export interface StockMovement {
  id: string;
  product_id: string;
  product_name: string | null;
  movement_type: StockMovementType;
  quantity: number;
  balance_before: number;
  balance_after: number;
  reference_type: string | null;
  reference_id: string | null;
  notes: string | null;
  timestamp: string;
  user_id: string;
  username: string;
  unit_id: string | null;
  unit_cost?: number | null;
}

export interface StockMovementFilters {
  product_id?: string;
  movement_type?: StockMovementType;
  start_date?: string;
  end_date?: string;
  reference_type?: string;
}

export interface StockMovementResponse {
  movements: StockMovement[];
  total_count: number;
  page: number;
  page_size: number;
  has_more: boolean;
}

export interface StockSummary {
  product_id: string;
  product_name: string;
  current_quantity: number;
  total_in: number;
  total_out: number;
  last_movement: string | null;
  movement_count: number;
}

// Unit Monthly Inventory Snapshot Types

export interface UnitMonthlySnapshot {
  id: string;
  unit_id: string;
  unit_name: string;
  report_year: number;
  report_month: number;
  product_id: string;
  product_name: string;
  // المعادلة: Opening + IN - OUT
  opening_stock: number;
  total_in: number;
  total_out: number;
  computed_closing: number;
  reported_closing: number;
  // Anomalies
  variance: number;
  has_balance_anomaly: boolean;
  avg_consumption_3months: number | null;
  has_consumption_anomaly: boolean;
  // Freshness
  is_stale: boolean;
  computed_at: string;
}

export interface UnitInventoryView {
  unit_id: string;
  unit_name: string;
  report_year: number;
  report_month: number;
  items: UnitMonthlySnapshot[];
  total_products: number;
  balance_anomaly_count: number;
  consumption_anomaly_count: number;
  stale_count: number;
  has_any_anomaly: boolean;
  computed_at: string;
}

export interface ComputeSnapshotResult {
  unit_id: string;
  unit_name: string;
  year: number;
  month: number;
  products_computed: number;
  balance_anomalies: number;
  consumption_anomalies: number;
  already_existed: boolean;
}

// WILAYA Reports
export interface WilayaReportList {
  type: 'Daily' | 'Monthly' | 'Stock';
  data: DailyReport[] | MonthlySummary[] | StockMovement[];
}

// ─── FIFO Inventory View Types (Phase 2) ──────────────────────────────────────

export interface InventoryLayerView {
  layer_id: string;
  source_type: string | null;
  received_at: string;
  qty_remaining: number;
  unit_cost: number;
  layer_value: number;
}

export interface InventoryProductView {
  product_id: string;
  product_name: string;
  total_quantity: number;
  total_value: number;
  oldest_layer_date: string | null;
  layer_count: number;
  layers: InventoryLayerView[];
}

export interface InventoryStockPageView {
  products: InventoryProductView[];
  total_inventory_value: number;
  total_products: number;
  total_active_layers: number;
}

// ─── Observability Types ──────────────────────────────────────────────────────

export interface AuditChainStatus {
  isValid: boolean;
  totalEntries: number;
  verifiedEntries: number;
  latestHash: string | null;
  latestEntryTimestamp: string | null;
  firstBrokenEntryId: string | null;
  breakDescription: string | null;
}

export interface AuditAnomaly {
  anomalyType: string;
  description: string;
  detectedAt: string;
  severity: 'INFO' | 'WARNING' | 'ERROR' | 'CRITICAL';
}

export interface AuditHealthReport {
  chainStatus: AuditChainStatus;
  lastImportTimestamp: string | null;
  lastExportTimestamp: string | null;
  lastBackupTimestamp: string | null;
  lastRestoreTimestamp: string | null;
  anomalies: AuditAnomaly[];
}

export type HealthStatus = 'HEALTHY' | 'DEGRADED' | 'CRITICAL' | 'UNKNOWN';

export interface ComponentHealth {
  status: HealthStatus;
  message: string;
  details: string | null;
}

export interface BackupHealth {
  status: HealthStatus;
  lastBackup: string | null;
  backupCount: number;
  backupAgeDays: number | null;
  message: string;
}

export interface SyncHealth {
  status: HealthStatus;
  failedImportsCount: number;
  failedDecryptionsCount: number;
  duplicatePackageAttempts: number;
  unresolvedConflicts: number;
  lastSuccessfulSync: string | null;
  message: string;
}

export interface SystemHealthReport {
  databaseStatus: ComponentHealth;
  backupStatus: BackupHealth;
  auditStatus: ComponentHealth;
  syncStatus: SyncHealth;
  storageUsageBytes: number;
  generatedAt: string;
}

export interface SyncNodeHealth {
  nodeId: string;
  nodeName: string;
  lastSyncTimestamp: string | null;
  packagesReceived: number;
  packagesRejected: number;
  replayAttempts: number;
  status: HealthStatus;
}

export interface ConflictResolutionSuggestion {
  action: string;
  description: string;
  requiresAdmin: boolean;
}

export interface SyncConflict {
  id: string;
  packageId: string;
  sourceNodeId: string;
  targetNodeId: string;
  conflictType: string;
  conflictTypeDisplay: string;
  severity: 'INFO' | 'WARNING' | 'ERROR' | 'CRITICAL';
  description: string;
  suggestedResolution: ConflictResolutionSuggestion | null;
  resolved: boolean;
  resolutionNote: string | null;
  resolvedBy: string | null;
  resolvedAt: string | null;
  createdAt: string;
}

export interface SeverityCount {
  severity: string;
  count: number;
}

export interface TypeCount {
  conflictType: string;
  conflictTypeDisplay: string;
  count: number;
}

export interface ConflictSummary {
  total: number;
  unresolved: number;
  bySeverity: SeverityCount[];
  byType: TypeCount[];
  recentConflicts: SyncConflict[];
}

// ─── Fiscal Closure Package Types ─────────────────────────────────────────────

export interface AuthorizedExecutionWindow {
  not_before: string;
  expires_at: string;
}

export interface FiscalClosurePackage {
  schema_version: number;
  closed_year: number;
  opened_year: number;
  closure_timestamp_utc: string;
  closure_authority_node_id: string;
  issuer_identity_id: string;
  closure_authority_username: string;
  fiscal_transition_id: string;
  package_created_at: string;
  signing_key_id: string | null;
  authorized_execution_window: AuthorizedExecutionWindow;
  package_fingerprint: string;
}

export interface FiscalClosurePreview {
  closed_year: number;
  opened_year: number;
  closure_timestamp_utc: string;
  closure_authority_node_id: string;
  closure_authority_username: string;
  fiscal_transition_id: string;
  package_created_at: string;
  signing_key_id: string | null;
  schema_version: number;
  authorized_execution_window: AuthorizedExecutionWindow;
  validation_ok: boolean;
  validation_issues: string[];
  package_fingerprint: string;
}

export interface FiscalClosureApplyResult {
  fiscal_transition_id: string;
  closed_year: number;
  opened_year: number;
  snapshot_count: number;
  package_fingerprint: string;
}

export interface FiscalTransitionHistoryEntry {
  transition_id: string;
  timestamp: string;
  action: string;
  fiscal_year: number;
  next_year: number;
  actor: string;
  execution_node: string | null;
  signing_key_id: string | null;
  package_fingerprint: string | null;
  source: 'AUTHORITY' | 'EXECUTION';
  status: string;
}

export interface FiscalPackageRegistryEntry {
  id: number;
  transition_id: string;
  fiscal_year: number;
  next_year: number;
  package_fingerprint: string;
  signing_key_id: string;
  exported_at: string;
  applied_at: string | null;
  retention_status: 'ACTIVE' | 'ARCHIVED' | 'RETIRED';
  exported_by: string;
  applied_by: string | null;
  archived: boolean;
  notes: string | null;
}

export interface FiscalYearStatus {
  year: number;
  status: 'open' | 'closed' | 'archived';
  opened_at?: string;
  closed_at?: string;
  closed_by?: string;
}

export interface BuildInfo {
  app_version: string;
  git_commit: string;
  build_timestamp: string;
  schema_version: number;
}
export type TelemetryEventType = 'BACKUP' | 'RESTORE' | 'SYNC_IMPORT' | 'SYNC_EXPORT' | 'INTEGRITY_CHECK' | 'WAL_CHECKPOINT' | 'RUNTIME_STARTUP';
export type TelemetryOutcome = 'SUCCESS' | 'FAILURE';

export interface TelemetryEvent {
  id: string;
  event_type: TelemetryEventType;
  outcome: TelemetryOutcome;
  duration_ms: number | null;
  timestamp: string;
  metadata: Record<string, unknown> | null;
  user_id: string | null;
  schema_version: number;
}
