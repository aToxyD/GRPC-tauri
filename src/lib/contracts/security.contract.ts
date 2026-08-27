import { safeInvoke } from '../tauri';

// Application-key provisioning (ADR-0041).
// Pre-auth lifecycle commands for Security Setup / Unlock. The passphrase and
// the raw identity never leave the backend; the encrypted artifact is written
// directly to disk via the secure path-parameterized export (SEC-017).

export interface AppKeyStatusDto {
  provisioned: boolean;
  unlocked: boolean;
  store_path: string;
  source: 'env' | 'keyring' | 'cache' | 'dev' | 'none';
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

export interface AppKeyImportResultDto {
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
 * operator's chosen destination. `remember` (default false, opt-in) persists
 * the VALIDATED App Key to the OS keyring for non-interactive startup
 * (ADR-0041 §11.4).
 */
export async function initializeAppKey(
  passphrase: string,
  exportBackup?: string | null,
  remember?: boolean,
): Promise<AppKeyInitializeResultDto> {
  return await safeInvoke('initialize_app_key', {
    passphrase,
    exportBackup: exportBackup ?? null,
    remember: remember ?? false,
  });
}

/**
 * Unlock the passphrase-protected `appkey.age` store (ADR-0041 §4 `Unlocked`).
 * Wrong passphrase / corrupt store stays locked (fail-closed). `remember`
 * (default false, opt-in) persists the validated App Key to the OS keyring;
 * a keyring failure never invalidates the unlock.
 */
export async function unlockAppKey(
  passphrase: string,
  remember?: boolean,
): Promise<AppKeyUnlockResultDto> {
  return await safeInvoke('unlock_app_key', { passphrase, remember: remember ?? false });
}

/**
 * Secure, path-parameterized App-Key backup re-export (SEC-017). Requires an
 * authenticated Admin session plus an unlocked store. The renderer sends ONLY
 * the destination path (from the native save dialog); the backend validates it
 * and writes the raw identity bytes directly to disk. The raw App-Key never
 * crosses IPC, the DOM, or logs — the promise resolves with no value.
 */
export async function exportAppKeyBackupToPath(filePath: string): Promise<void> {
  return await safeInvoke('export_app_key_backup_to_path', { filePath });
}

/**
 * Import the WILAYA-sourced portable artifact `grpc-app-key.age` into the
 * encrypted local store (APPKEY-003, ADR-0041 §10.4 amendment). The renderer
 * sends only the artifact path; the backend reads, validates, and encrypts it.
 * The raw identity never crosses IPC, the DOM, or logs. `remember` (default
 * false, opt-in) persists the validated App Key to the OS keyring.
 */
export async function importAppKey(
  passphrase: string,
  artifactPath: string,
  remember?: boolean,
): Promise<AppKeyImportResultDto> {
  return await safeInvoke('import_app_key', {
    passphrase,
    artifactPath,
    remember: remember ?? false,
  });
}

/**
 * Remove the remembered App Key from the OS keyring (ADR-0041 §11.4).
 * Device-local preference only — `appkey.age`, the database, identities,
 * users, sessions, and provisioning state are untouched. Returns `true` if a
 * remembered entry was removed, `false` if none existed.
 */
export async function forgetRememberedAppKey(): Promise<boolean> {
  return await safeInvoke('forget_remembered_app_key');
}
