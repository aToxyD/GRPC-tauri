import { safeInvoke } from '../tauri';
import type {
  LoginMetrics, SystemMetrics, SyncSecurityDiagnostics, SyncPreflightCheck,
} from '../types';

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
