import { test as base, expect } from '@playwright/test';
import { TauriDriver } from '../drivers/tauriDriver';

type TauriAppFixtures = {
  tauriApp: {
    driver: TauriDriver;
    page: import('@playwright/test').Page;
  };
};

export const test = base.extend<TauriAppFixtures>({
  tauriApp: async ({}, use) => {
    const driver = new TauriDriver();
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
