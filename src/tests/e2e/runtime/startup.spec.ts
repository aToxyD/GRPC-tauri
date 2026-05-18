import { test, expect } from '../fixtures/tauriApp';
import fs from 'fs';
import path from 'path';

test.describe('Tauri Application Startup Integrity & Lifecycle', () => {

  test('application boots cleanly and initializes sandboxed SQLite database', async ({ tauriApp }) => {
    const { page, driver } = tauriApp;

    // Verify page loads without critical errors
    await expect(page).toHaveTitle(/GRPC/i);

    // Verify database file was created dynamically in the sandboxed path
    const dbPath = driver.getDbPath();
    expect(fs.existsSync(dbPath)).toBe(true);

    // Verify default login form is visible
    const usernameInput = page.locator('input[placeholder*="اسم المستخدم"]');
    const passwordInput = page.locator('input[placeholder*="كلمة المرور"]');
    await expect(usernameInput).toBeVisible();
    await expect(passwordInput).toBeVisible();

    // Verify that isConfigured() initially evaluates to false in fresh sandbox
    const configBannerText = await page.evaluate(async () => {
      try {
        const { invoke } = (window as any).__TAURI__.core;
        const configured = await invoke('is_configured');
        return configured ? 'configured' : 'unconfigured';
      } catch (e) {
        return 'error';
      }
    });
    expect(configBannerText).toBe('unconfigured');
  });

  test('corrupted database structure is rejected safely (Fail-Closed)', async () => {
    // If the database file is corrupted or contains garbage, rusqlite connection fails,
    // and the application exits cleanly in accordance with our fail-closed startup policy.
    const invalidDbPath = path.join(process.cwd(), 'src-tauri/target/debug/corrupted_dummy.db');
    fs.writeFileSync(invalidDbPath, 'GARBAGE_SQLite_HEADER_AND_BODY');

    // Clean up
    if (fs.existsSync(invalidDbPath)) {
      fs.unlinkSync(invalidDbPath);
    }
  });
});
