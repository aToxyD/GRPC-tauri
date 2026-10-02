import { test, expect } from '../fixtures/tauriApp';
import { RuntimeContracts } from '../contracts/runtimeContracts';
import { ensureLoginIdentity } from '../helpers/login';
import { performFirstAdminCeremony } from '../helpers/firstAdminCeremony';

test.describe('Tauri Authentication Runtime Lifecycle & Lockout', () => {
  test.skip(process.platform !== 'win32', 'Tauri IPC backend is required and only supported on Windows in E2E tests');

  test('rate limiter blocks access after 5 consecutive invalid login attempts', async ({ tauriApp }) => {
    const { page } = tauriApp;

    await ensureLoginIdentity(page, 'admin');
    const passwordInput = page.locator('input[placeholder*="كلمة المرور"]');
    const submitBtn = page.locator('button:has-text("تسجيل الدخول")');

    // Fill in incorrect details 5 times — the identity is fixed (ADR-0052),
    // only the wrong credential rotates per attempt.
    for (let i = 0; i < 5; i++) {
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

  test('successful authentication, session bootstrap, and node redirection', async ({ tauriAdminApp }) => {
    const { page, driver, rootCeremony } = tauriAdminApp;

    // Real first-ADMIN bootstrap ceremony: Root-sign the WILAYA CSR, issue the
    // admin identity key, authenticate through the admin-key UI, and configure
    // the node. Production seeds no password-enabled admin, so a password login
    // cannot reach an authenticated state on a fresh database.
    const ceremony = await performFirstAdminCeremony({ page, driver, rootCeremony });

    // The ceremony leaves the node authenticated, configured, and on the
    // dashboard; assert the identities it actually provisioned.
    expect(ceremony.wilayaCertificate.subject_type).toBe('WILAYA');
    expect(ceremony.wilayaCertificate.status).toBe('ACTIVE');
    expect(ceremony.adminCertificate.subject_type).toBe('ADMIN');

    // Assert session exists in memory/local storage
    await RuntimeContracts.assertSecureSessionBootstrapped(page);
  });
});
