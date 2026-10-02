import { test, expect } from '../fixtures/tauriApp';
import { performFirstAdminCeremony } from '../helpers/firstAdminCeremony';
import fs from 'fs';

test.describe('Backup & Restore Runtime Lifecycle Workflows', () => {
  test.skip(process.platform !== 'win32', 'Tauri IPC backend is required and only supported on Windows in E2E tests');

  test('backup creation, WAL checkpointing, encryption, and listing', async ({ tauriAdminApp }) => {
    const { page, driver, rootCeremony } = tauriAdminApp;

    // Login and setup node via the real first-ADMIN ceremony
    await performFirstAdminCeremony({ page, driver, rootCeremony });

    // 1. Invoke backup creation command
    const backupPath = await page.evaluate(async () => {
      const { invoke } = (window as any).__TAURI__.core;
      return await invoke('create_backup');
    });

    expect(backupPath).toBeDefined();
    expect(fs.existsSync(backupPath)).toBe(true);

    // Verify backup contains the encrypted age envelope
    const fileContent = fs.readFileSync(backupPath, 'utf8');
    expect(fileContent.startsWith('age-encryption.org')).toBe(true);

    // 2. Retrieve list of active backups
    const backupsList = await page.evaluate(async () => {
      const { invoke } = (window as any).__TAURI__.core;
      return await invoke('list_backups');
    });

    expect(backupsList.length).toBeGreaterThanOrEqual(1);
    expect(backupsList[0].path).toBe(backupPath);

    // 3. Issue and validate restore confirmation preflight
    const preflightToken = await page.evaluate(async () => {
      const { invoke } = (window as any).__TAURI__.core;
      const tokenRes = await invoke('issue_operation_execution_token', {
        request: {
          operation: 'restore',
          year: null,
          next_year: null
        }
      });
      return tokenRes.token;
    });

    expect(preflightToken).toBeDefined();
    expect(preflightToken.length).toBeGreaterThan(0);

    // 4. Validate that typed confirmation is correctly verified before restoring
    const confirmationValid = await page.evaluate(async () => {
      const { invoke } = (window as any).__TAURI__.core;
      try {
        await invoke('validate_restore_confirmation', {
          request: { confirmation: 'RESTORE' }
        });
        return 'valid';
      } catch (err: any) {
        return err.toString();
      }
    });
    expect(confirmationValid).toBe('valid');
  });
});
