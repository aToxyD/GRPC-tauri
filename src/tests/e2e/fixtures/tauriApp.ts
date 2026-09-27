import { test as base, expect } from '@playwright/test';
import { TauriDriver } from '../drivers/tauriDriver';
import { ViteDriver } from '../drivers/viteDriver';
import fs from 'fs';

type TauriAppFixtures = {
  tauriApp: {
    driver: TauriDriver | ViteDriver;
    page: import('@playwright/test').Page;
  };
};

export const test = base.extend<TauriAppFixtures>({
  tauriApp: async ({}, use, testInfo) => {
    const driver = process.platform === 'win32' ? new TauriDriver() : new ViteDriver();
    let failed = false;
    try {
      const page = await driver.start();

      page.on('console', msg => {
        console.log(`[BROWSER CONSOLE] [${msg.type()}] ${msg.text()}`);
      });

      // Inject custom test environment attributes to the window
      try {
        await page.evaluate(() => {
          (window as any).__E2E_TEST_ACTIVE__ = true;
        });
      } catch (e) {}

      await use({ driver, page });
    } catch (e) {
      failed = true;
      throw e;
    } finally {
      // Attach diagnostics BEFORE stop(), which deletes the temp dir holding
      // the app stdout/stderr log. Without this the app's own output is lost.
      try {
        await testInfo.attach('tauri-diagnostics.txt', {
          body: driver.getDiagnostics(),
          contentType: 'text/plain',
        });
        const logPath = driver.getRunLogPath();
        if (logPath && fs.existsSync(logPath) && (failed || testInfo.status === 'failed')) {
          await testInfo.attach('tauri_run.log', {
            path: logPath,
            contentType: 'text/plain',
          });
        }
      } catch (e) {}
      await driver.stop();
    }
  },
});

export { expect };
