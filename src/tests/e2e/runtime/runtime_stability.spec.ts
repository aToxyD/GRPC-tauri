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
    for (let i = 0; i < 5; i++) {
      await submitBtn.click({ clickCount: 2, delay: 20 }).catch(() => {});
      await page.waitForTimeout(100);
    }

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
