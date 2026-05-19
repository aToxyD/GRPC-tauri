import { test, expect } from '../fixtures/tauriApp';
import { RuntimeContracts } from '../contracts/runtimeContracts';

test.describe('Real IPC Validation & Filesystem Sandboxing', () => {

  test('IPC commands enforce node boundaries and reject path traversal attempts', async ({ tauriApp }) => {
    const { page } = tauriApp;

    // Login and configure to initialize database settings context
    await page.locator('input[placeholder*="اسم المستخدم"]').fill('admin');
    await page.locator('input[placeholder*="كلمة المرور"]').fill('admin');
    await page.locator('button:has-text("تسجيل الدخول")').click();

    await page.waitForSelector('h1:has-text("تكوين الولاية")');
    await page.locator('input[id="wilayaCode"]').fill('16');
    await page.locator('input[id="wilayaName"]').fill('الجزائر العاصمة');
    await page.locator('button:has-text("تكوين كولاية")').click();
    await page.waitForSelector('h1:has-text("لوحة تحكم الولاية")');

    // 1. Validate that direct call to unauthorized commands fails
    await RuntimeContracts.assertIpcGuardsEnforced(page);

    // 2. Validate filesystem sandbox rejection for traversal paths
    await RuntimeContracts.assertPathTraversalRejected(page);

    // 3. Test that legitimate IPC queries like fetching settings return structured data
    const settings = await page.evaluate(async () => {
      for (let i = 0; i < 50; i++) {
        if ((window as any).__TAURI__ && (window as any).__TAURI__.core) break;
        await new Promise(r => setTimeout(r, 100));
      }
      const { invoke } = (window as any).__TAURI__.core;
      return await invoke('get_settings');
    });

    expect(settings).toBeDefined();
    expect(settings.node_type).toBe('WILAYA');
    expect(settings.configured).toBe(true);
  });

  test('malformed IPC payloads are rejected gracefully by Rust types', async ({ tauriApp }) => {
    const { page } = tauriApp;

    await page.waitForSelector('input[placeholder*="اسم المستخدم"]');
    // Send malformed payload to a command and assert error propagation
    const ipcError = await page.evaluate(async () => {
      for (let i = 0; i < 50; i++) {
        if ((window as any).__TAURI__ && (window as any).__TAURI__.core) break;
        await new Promise(r => setTimeout(r, 100));
      }
      try {
        const { invoke } = (window as any).__TAURI__.core;
        // invoke expects string parameters but we send numbers or empty objects
        await invoke('configure_as_wilaya', { wilayaCode: 12345, wilayaName: null });
        return 'success';
      } catch (err: any) {
        return err.toString();
      }
    });

    expect(ipcError).not.toBe('success');
    expect(ipcError).toContain('invalid type');
  });
});
