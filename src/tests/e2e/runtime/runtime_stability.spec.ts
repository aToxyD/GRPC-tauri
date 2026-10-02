import { test, expect } from '../fixtures/tauriApp';
import { ensureLoginIdentity } from '../helpers/login';

test.describe('Runtime stability', () => {
  test('login page survives rapid submit clicks without runtime errors', async ({ tauriApp }) => {
    const { page } = tauriApp;
    const runtimeErrors: string[] = [];
    page.on('pageerror', (err) => runtimeErrors.push(err.message));

    await page.waitForSelector('button:has-text("تسجيل الدخول")');
    const submitBtn = page.locator('button:has-text("تسجيل الدخول")');
    const passwordInput = page.locator('input[placeholder*="كلمة المرور"]');

    await ensureLoginIdentity(page, 'admin');
    await passwordInput.fill('wrong');
    // The login page arms a client-side latch once 5 attempts fail
    // (LoginPage.svelte:432) that disables the submit button
    // (LoginPage.svelte:576) and is cleared only by a successful login
    // (LoginPage.svelte:350). Reaching that latch is therefore the expected
    // outcome of a rapid-submit run, not a runtime error.
    //
    // Two bounds keep this deterministic instead of stalling until the 90 s
    // test timeout: global `actionTimeout` is 0 (playwright.config.ts:15), so
    // every click needs an explicit deadline, and clicks must stop being issued
    // once the latch disables the button.
    let attempts = 0;
    for (let i = 0; i < 5; i++) {
      if (await submitBtn.isDisabled()) break;
      await submitBtn.click({ clickCount: 2, delay: 20, timeout: 5_000 });
      attempts++;
      await page.waitForTimeout(100);
    }

    // Guard against a vacuous pass where no submission ever ran.
    expect(attempts).toBeGreaterThan(0);
    // Bounded wait for the documented lockout state to settle.
    await expect(submitBtn).toBeDisabled({ timeout: 15_000 });

    expect(runtimeErrors).toEqual([]);
  });

  test('hash navigation between login routes does not throw', async ({ tauriApp }) => {
    const { page } = tauriApp;
    const runtimeErrors: string[] = [];
    page.on('pageerror', (err) => runtimeErrors.push(err.message));

    await page.waitForSelector('button:has-text("تسجيل الدخول")');
    for (const hash of ['#/login', '#/', '#/login']) {
      await page.evaluate((h) => {
        window.location.hash = h;
      }, hash);
      await page.waitForTimeout(200);
    }

    expect(runtimeErrors).toEqual([]);
  });
});
