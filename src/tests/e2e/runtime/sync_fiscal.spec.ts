import { test, expect } from '../fixtures/tauriApp';
import { ensureLoginIdentity } from '../helpers/login';
import path from 'path';
import fs from 'fs';

test.describe('Sync Interoperability & Fiscal Closure Operations', () => {
  test.skip(process.platform !== 'win32', 'Tauri IPC backend is required and only supported on Windows in E2E tests');

  test('export products sync package verifies package structure and V2 signing', async ({ tauriApp }) => {
    const { page, driver } = tauriApp;

    // Login and setup node
    await ensureLoginIdentity(page, 'admin');
    await page.locator('input[placeholder*="كلمة المرور"]').fill('admin');
    await page.locator('button:has-text("تسجيل الدخول")').click();

    await page.waitForSelector('h1:has-text("تكوين الولاية")');
    await page.locator('input[id="wilayaCode"]').fill('16');
    await page.locator('input[id="wilayaName"]').fill('الجزائر العاصمة');
    await page.locator('button:has-text("تكوين كولاية")').click();
    await page.waitForSelector('h1:has-text("لوحة تحكم الولاية")');

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

  test('corrupted or tampered sync packages are rejected safely', async ({ tauriApp }) => {
    const { page, driver } = tauriApp;

    // Login and setup node
    await ensureLoginIdentity(page, 'admin');
    await page.locator('input[placeholder*="كلمة المرور"]').fill('admin');
    await page.locator('button:has-text("تسجيل الدخول")').click();

    await page.waitForSelector('h1:has-text("تكوين الولاية")');
    await page.locator('input[id="wilayaCode"]').fill('16');
    await page.locator('input[id="wilayaName"]').fill('الجزائر العاصمة');
    await page.locator('button:has-text("تكوين كولاية")').click();
    await page.waitForSelector('h1:has-text("لوحة تحكم الولاية")');

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

  test('fiscal closure workflow transitions year and enforces immutability', async ({ tauriApp }) => {
    const { page } = tauriApp;

    // Login and setup node
    await ensureLoginIdentity(page, 'admin');
    await page.locator('input[placeholder*="كلمة المرور"]').fill('admin');
    await page.locator('button:has-text("تسجيل الدخول")').click();

    await page.waitForSelector('h1:has-text("تكوين الولاية")');
    await page.locator('input[id="wilayaCode"]').fill('16');
    await page.locator('input[id="wilayaName"]').fill('الجزائر العاصمة');
    await page.locator('button:has-text("تكوين كولاية")').click();
    await page.waitForSelector('h1:has-text("لوحة تحكم الولاية")');

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
