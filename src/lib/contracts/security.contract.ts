import { safeInvoke } from '../tauri';

// Application-key provisioning (ADR-0041).
// Pre-auth lifecycle commands for Security Setup / Unlock. The passphrase never
// leaves the backend; the raw identity only crosses the boundary on an explicit
// opt-in backup export.

export interface AppKeyStatusDto {
  provisioned: boolean;
  unlocked: boolean;
  store_path: string;
  source: 'env' | 'store' | 'dev' | 'none';
  requires_action: boolean;
}

export interface AppKeyInitializeResultDto {
  provisioned: boolean;
  unlocked: boolean;
  store_path: string;
  exported_backup: boolean;
}

export interface AppKeyUnlockResultDto {
  provisioned: boolean;
  unlocked: boolean;
  store_path: string;
}

/** Live provisioning status (ADR-0041 §9). */
export async function getSecurityStatus(): Promise<AppKeyStatusDto> {
  return await safeInvoke('get_security_status');
}

/**
 * First-run key generation (Security Setup, ADR-0041 §6): generate a fresh
 * x25519 identity, wrap it with the passphrase, persist `appkey.age`, and boot
 * the database. `exportBackup` optionally writes the raw identity once to the
 * operator's chosen destination.
 */
export async function initializeAppKey(
  passphrase: string,
  exportBackup?: string | null,
): Promise<AppKeyInitializeResultDto> {
  return await safeInvoke('initialize_app_key', {
    passphrase,
    export_backup: exportBackup ?? null,
  });
}

/**
 * Unlock the passphrase-protected `appkey.age` store (ADR-0041 §4 `Unlocked`).
 * Wrong passphrase / corrupt store stays locked (fail-closed).
 */
export async function unlockAppKey(passphrase: string): Promise<AppKeyUnlockResultDto> {
  return await safeInvoke('unlock_app_key', { passphrase });
}

/** Guarded re-export of the raw application identity (requires unlocked store). */
export async function exportAppKeyBackup(): Promise<string> {
  return await safeInvoke('export_app_key_backup');
}
