import { test, expect } from '../fixtures/tauriApp';
import { RuntimeContracts } from '../contracts/runtimeContracts';

test.describe('Tauri Authentication Runtime Lifecycle & Lockout', () => {

  test('rate limiter blocks access after 5 consecutive invalid login attempts', async ({ tauriApp }) => {
    const { page } = tauriApp;

    const usernameInput = page.locator('input[placeholder*="اسم المستخدم"]');
    const passwordInput = page.locator('input[placeholder*="كلمة المرور"]');
    const submitBtn = page.locator('button:has-text("تسجيل الدخول")');

    // Fill in incorrect details 5 times
    for (let i = 0; i < 5; i++) {
      await usernameInput.fill('admin');
      await passwordInput.fill(`wrong_password_${i}`);
      await submitBtn.click();

      // Wait for IPC return and UI update
      await expect(page.locator('text=محاولات تسجيل الدخول:')).toContainText(`${i + 1}/5`);
    }

    // Verify rate limit lockout propagation
    const errorLocators = page.locator('.bg-red-50, .text-red-700');
    await expect(errorLocators.first()).toBeVisible();
    const texts = await errorLocators.allInnerTexts();
    const joinedText = texts.join(' | ');
    expect(joinedText).toMatch(/محظور|تجاوز الحد/);
  });

  test('successful authentication, session bootstrap, and node redirection', async ({ tauriApp }) => {
    const { page } = tauriApp;

    // Login as default admin
    await page.locator('input[placeholder*="اسم المستخدم"]').fill('admin');
    await page.locator('input[placeholder*="كلمة المرور"]').fill('admin');
    await page.locator('button:has-text("تسجيل الدخول")').click();

    // Since this is a fresh database, it redirects to the node setup page
    await page.waitForSelector('h1:has-text("تكوين الولاية")');
    expect(page.url()).toContain('configure');

    // Perform configuration as Wilaya node
    await page.locator('input[id="wilayaCode"]').fill('16');
    await page.locator('input[id="wilayaName"]').fill('الجزائر العاصمة');
    await page.locator('button:has-text("تكوين كولاية")').click();

    // Redirection to main Wilaya dashboard
    await page.waitForSelector('h1:has-text("لوحة تحكم الولاية")');
    expect(page.url()).toContain('wilaya');

    // Assert session exists in memory/local storage
    await RuntimeContracts.assertSecureSessionBootstrapped(page);
  });
});
