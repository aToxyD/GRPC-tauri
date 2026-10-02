import { test, expect } from '../fixtures/tauriApp';
import { performFirstAdminCeremony } from '../helpers/firstAdminCeremony';
import { RuntimeContracts } from '../contracts/runtimeContracts';

test.describe('Real IPC Validation & Filesystem Sandboxing', () => {
  test.skip(process.platform !== 'win32', 'Tauri IPC backend is required and only supported on Windows in E2E tests');

  test('IPC commands enforce node boundaries and reject path traversal attempts', async ({ tauriAdminApp }) => {
    const { page, driver, rootCeremony } = tauriAdminApp;

    // Login and configure to initialize database settings context
    await performFirstAdminCeremony({ page, driver, rootCeremony });

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

    await page.waitForSelector('#username');
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
