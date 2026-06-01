import { safeInvoke } from '../tauri';
import type {
  AuditChainStatus, AuditHealthReport, SystemHealthReport,
  SyncNodeHealth, ConflictSummary, SyncConflict,
} from '../types';

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
