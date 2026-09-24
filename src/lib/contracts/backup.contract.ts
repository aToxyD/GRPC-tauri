import { safeInvoke } from '../tauri';
import type { BackupInfo } from '../types';

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
  olderStateConfirmation?: string | null,
): Promise<void> {
  return await safeInvoke('restore_backup', {
    backupPath,
    confirmation,
    executionToken,
    olderStateConfirmation: olderStateConfirmation ?? null,
  });
}
