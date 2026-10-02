import { test as base, expect } from '@playwright/test';
import type { Page, TestInfo } from '@playwright/test';
import { TauriDriver } from '../drivers/tauriDriver';
import { ViteDriver } from '../drivers/viteDriver';
import { createRootCeremony, type RootCeremony } from '../helpers/firstAdminCeremony';
import fs from 'fs';

type TauriAppFixtures = {
  tauriApp: {
    driver: TauriDriver | ViteDriver;
    page: Page;
  };
  /**
   * Same application lifecycle as `tauriApp`, except the Authority Root keypair
   * is generated first and only its PUBLIC key is handed to the process as
   * `GRPC_ROOT_PUBLIC_KEY`.
   *
   * This must happen before launch: `resolve_root_public_key()` reads the
   * process environment, and a running Windows process cannot receive new
   * environment variables. Every test that drives the real first-admin ceremony
   * therefore depends on this fixture instead of `tauriApp`.
   */
  tauriAdminApp: {
    driver: TauriDriver;
    page: Page;
    rootCeremony: RootCeremony;
  };
};

/**
 * Attaches the app lifecycle diagnostics to the Playwright report.
 *
 * Must run BEFORE `stop()`, which deletes the temp dir holding the app's
 * stdout/stderr log. Without this the app's own output is lost.
 */
async function attachTauriDiagnostics(
  driver: TauriDriver | ViteDriver,
  testInfo: TestInfo,
  attachRunLog: boolean
): Promise<void> {
  try {
    await testInfo.attach('tauri-diagnostics.txt', {
      body: driver.getDiagnostics(),
      contentType: 'text/plain',
    });
    const logPath = driver.getRunLogPath();
    if (attachRunLog && logPath && fs.existsSync(logPath)) {
      await testInfo.attach('tauri_run.log', {
        path: logPath,
        contentType: 'text/plain',
      });
    }
  } catch (e) {}
}

async function armTestPage(page: Page): Promise<void> {
  page.on('console', msg => {
    console.log(`[BROWSER CONSOLE] [${msg.type()}] ${msg.text()}`);
  });

  // Inject custom test environment attributes to the window
  try {
    await page.evaluate(() => {
      (window as any).__E2E_TEST_ACTIVE__ = true;
    });
  } catch (e) {}
}

export const test = base.extend<TauriAppFixtures>({
  tauriApp: async ({}, use, testInfo) => {
    const driver = process.platform === 'win32' ? new TauriDriver() : new ViteDriver();
    let failed = false;
    try {
      const page = await driver.start();
      await armTestPage(page);

      await use({ driver, page });
    } catch (e) {
      failed = true;
      throw e;
    } finally {
      await attachTauriDiagnostics(driver, testInfo, failed || testInfo.status === 'failed');
      await driver.stop();
    }
  },

  tauriAdminApp: async ({}, use, testInfo) => {
    // Generate the throwaway Authority Root keypair BEFORE spawning the app.
    const rootCeremony = createRootCeremony();
    const driver = new TauriDriver();
    let failed = false;
    try {
      // Only the public half crosses into the application environment. The
      // private key stays inside the ceremony directory and is only ever read
      // by `root-signer --key-file`.
      const page = await driver.start({ GRPC_ROOT_PUBLIC_KEY: rootCeremony.publicKey });
      await armTestPage(page);

      await use({ driver, page, rootCeremony });
    } catch (e) {
      failed = true;
      throw e;
    } finally {
      await attachTauriDiagnostics(driver, testInfo, failed || testInfo.status === 'failed');
      try {
        await driver.stop();
      } finally {
        // ProcessManager only owns the application sandbox; the ceremony directory
        // holds the Root private key and must be removed explicitly. Nested in a
        // `finally` so an unexpected `stop()` failure cannot skip it and leave
        // the Root private key behind in the temp directory.
        rootCeremony.cleanup();
      }
    }
  },
});

export { expect };
