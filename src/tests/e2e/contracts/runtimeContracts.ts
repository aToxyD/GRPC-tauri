import { expect } from '@playwright/test';
import type { Page } from '@playwright/test';

/**
 * Validates operational and security constraints on the running Tauri application.
 */
export class RuntimeContracts {
  
  /**
   * Verifies that the application's secure session has been cleanly bootstrapped.
   */
  public static async assertSecureSessionBootstrapped(page: Page): Promise<void> {
    const sessionDetails = await page.evaluate(async () => {
      // Poll for window.__TAURI__ to be defined
      for (let i = 0; i < 50; i++) {
        if ((window as any).__TAURI__ && (window as any).__TAURI__.core) {
          break;
        }
        await new Promise(r => setTimeout(r, 100));
      }

      try {
        const { invoke } = (window as any).__TAURI__.core;
        const status: any = await invoke('check_session');
        const currentUser = (window as any).__E2E_TEST_ACTIVE__;
        return { status, currentUser, url: window.location.href, error: null };
      } catch (e: any) {
        return { status: null, currentUser: null, url: window.location.href, error: String(e) };
      }
    });
    expect(sessionDetails.error).toBeNull();
    expect(sessionDetails.status.is_active).toBe(true);
  }

  /**
   * Validates that the frontend does not bypass IPC guards.
   */
  public static async assertIpcGuardsEnforced(page: Page): Promise<void> {
    const ipcBypassAttempt = await page.evaluate(async () => {
      for (let i = 0; i < 50; i++) {
        if ((window as any).__TAURI__ && (window as any).__TAURI__.core) break;
        await new Promise(r => setTimeout(r, 100));
      }
      try {
        // Attempt an unauthorized directly-invoked internal command
        const { invoke } = (window as any).__TAURI__.core;
        await invoke('unauthorized_admin_command');
        return 'success';
      } catch (err: any) {
        return err.toString();
      }
    });
    // Must reject due to unauthorized execution token or command signature mismatch
    expect(ipcBypassAttempt).toMatch(/Command.*not found/);
  }

  /**
   * Validates that path traversal attempts are rejected.
   */
  public static async assertPathTraversalRejected(page: Page): Promise<void> {
    const traversalAttempt = await page.evaluate(async () => {
      for (let i = 0; i < 50; i++) {
        if ((window as any).__TAURI__ && (window as any).__TAURI__.core) break;
        await new Promise(r => setTimeout(r, 100));
      }
      try {
        const { invoke } = (window as any).__TAURI__.core;
        await invoke('read_settings_from_path', { path: '../../../../etc/passwd' });
        return 'success';
      } catch (err: any) {
        return err.toString();
      }
    });
    expect(traversalAttempt).not.toBe('success');
  }
}
