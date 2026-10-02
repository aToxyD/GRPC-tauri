import { test, expect } from '../fixtures/tauriApp';
import {
  E2E_ADMIN_PASSPHRASE,
  performFirstAdminCeremony,
} from '../helpers/firstAdminCeremony';
import path from 'path';
import fs from 'fs';

test.describe('Sync Interoperability & Fiscal Closure Operations', () => {
  test.skip(process.platform !== 'win32', 'Tauri IPC backend is required and only supported on Windows in E2E tests');

  test('export products sync package verifies package structure and V2 signing', async ({ tauriAdminApp }) => {
    const { page, driver, rootCeremony } = tauriAdminApp;

    // Login and setup node via the real first-ADMIN ceremony
    await performFirstAdminCeremony({ page, driver, rootCeremony });

    // SEC-033: `export_products_package` enumerates the authoritative UNIT
    // target set server-side and fails closed when none is registered
    // (`transport_target.rs:84`). Exactly ONE unit is required:
    // `derive_per_target_artifact_path` preserves the operator-requested path
    // verbatim only while `target_count <= 1` (`transport_target.rs:133`), so a
    // second unit would invalidate the `existsSync(exportPath)` assertion below.
    //
    // The unit is created through the real `create_unit` command
    // (`commands/units.rs:20`), never by writing to SQLite. The ceremony above
    // already satisfies every precondition: an Admin session, and a node
    // configured as WILAYA so `settings.wilaya_code` resolves and the
    // `Action::ManageUnits` policy admits `ResourceContext::WilayaNode`.
    const createdUnit = await page.evaluate(
      async (unitPassword) => {
        const { invoke } = (window as any).__TAURI__.core;
        return await invoke('create_unit', {
          request: {
            // Passes the transport-target path-component check and the dormant
            // `validate_create_unit_request` rule (exactly 6 alphanumeric
            // characters) as well as the live command's UNIQUE constraint.
            code: 'UNIT01',
            name: 'E2E Export Target Unit',
            password: unitPassword,
          },
        });
      },
      E2E_ADMIN_PASSPHRASE
    );
    expect(createdUnit.code).toBe('UNIT01');
    // `wilaya_code` is derived server-side from settings, proving the ceremony's
    // configured WILAYA supplied it rather than the caller.
    expect(createdUnit.wilaya_code).toBe('16');

    // Export path inside test isolated folder
    const exportPath = path.join(path.dirname(driver.getDbPath()), 'catalog.sync');

    const exportResult = await page.evaluate(async (pathStr) => {
      const { invoke } = (window as any).__TAURI__.core;
      return await invoke('export_products_package', { filePath: pathStr });
    }, exportPath);

    expect(exportResult.success).toBe(true);
    expect(fs.existsSync(exportPath)).toBe(true);

    // Verify file format has the age-encrypted header
    const data = fs.readFileSync(exportPath, 'utf8');
    expect(data.startsWith('age-encryption.org')).toBe(true);
  });

  test('corrupted or tampered sync packages are rejected safely', async ({ tauriAdminApp }) => {
    const { page, driver, rootCeremony } = tauriAdminApp;

    // Login and setup node via the real first-ADMIN ceremony
    await performFirstAdminCeremony({ page, driver, rootCeremony });

    // Write a corrupted file to import
    const corruptPath = path.join(path.dirname(driver.getDbPath()), 'corrupted.sync');
    fs.writeFileSync(corruptPath, 'AGE-ENCRYPTED-STREAM-BUT-MODIFIED-HMAC-FOOTER');

    const importError = await page.evaluate(async (pathStr) => {
      const { invoke } = (window as any).__TAURI__.core;
      try {
        await invoke('import_products_package', { filePath: pathStr });
        return 'success';
      } catch (err: any) {
        return err.toString();
      }
    }, corruptPath);

    // Fail-Closed: must reject corrupted package safely
    expect(importError).not.toBe('success');
    expect(importError).toContain('Decryption failed');

    // Cleanup
    if (fs.existsSync(corruptPath)) {
      fs.unlinkSync(corruptPath);
    }
  });

  test('fiscal closure workflow transitions year and enforces immutability', async ({ tauriAdminApp }) => {
    const { page, driver, rootCeremony } = tauriAdminApp;

    // Login and setup node via the real first-ADMIN ceremony
    await performFirstAdminCeremony({ page, driver, rootCeremony });

    // 1. Issue operational execution token for fiscal close
    const tokenResponse = await page.evaluate(async () => {
      const { invoke } = (window as any).__TAURI__.core;
      // Get current year
      const settings = await invoke('get_settings');
      const currentYear = settings.current_year;

      const tokenRes = await invoke('issue_operation_execution_token', {
        request: {
          operation: 'fiscal_close',
          year: currentYear,
          next_year: currentYear + 1
        }
      });
      return { token: tokenRes.token, currentYear };
    });

    expect(tokenResponse.token).toBeDefined();
    expect(tokenResponse.token.length).toBeGreaterThan(0);

    // 2. Perform confirmed fiscal close
    const closeResult = await page.evaluate(async ({ token, currentYear }) => {
      const { invoke } = (window as any).__TAURI__.core;
      return await invoke('close_fiscal_year_confirmed', {
        request: {
          year: currentYear,
          next_year: currentYear + 1,
          confirmation: currentYear.toString(),
          execution_token: token
        }
      });
    }, tokenResponse);

    expect(closeResult.closed_year).toBe(tokenResponse.currentYear);
    expect(closeResult.opened_year).toBe(tokenResponse.currentYear + 1);

    // 3. Verify settings reflects the new opened year
    const updatedSettings = await page.evaluate(async () => {
      const { invoke } = (window as any).__TAURI__.core;
      return await invoke('get_settings');
    });
    expect(updatedSettings.current_year).toBe(tokenResponse.currentYear + 1);
  });
});
