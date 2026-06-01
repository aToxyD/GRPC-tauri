import { safeInvoke } from '../tauri';
import type {
  AuditFilters, AuditLogResponse, AuditStats, AuditEntry, XlsxExportResult,
} from '../types';

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
