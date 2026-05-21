import { test as base, expect } from '@playwright/test';
import { TauriDriver } from '../drivers/tauriDriver';
import { ViteDriver } from '../drivers/viteDriver';

type TauriAppFixtures = {
  tauriApp: {
    driver: TauriDriver | ViteDriver;
    page: import('@playwright/test').Page;
  };
};

export const test = base.extend<TauriAppFixtures>({
  tauriApp: async ({}, use) => {
    const driver = process.platform === 'win32' ? new TauriDriver() : new ViteDriver();
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
    
    await driver.stop();
  },
});

export { expect };
