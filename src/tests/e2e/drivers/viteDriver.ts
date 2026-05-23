import { chromium } from '@playwright/test';
import type { ChromiumBrowser, BrowserContext, Page } from '@playwright/test';

/**
 * ViteDriver connects Playwright to the Vite dev server instead of a real Tauri binary.
 *
 * Background: On macOS, Tauri uses WKWebView which does NOT expose a Chrome DevTools
 * Protocol (CDP) remote-debugging port, unlike Windows WebView2. Consequently,
 * chromium.connectOverCDP() cannot be used on macOS.
 *
 * ViteDriver launches a standard Playwright Chromium browser, injects a minimal
 * __TAURI_INTERNALS__ shim so the @tauri-apps/api/core SDK does not throw TypeError
 * when invoked in a plain browser context, and navigates to the Vite dev server.
 *
 * All IPC calls (invoke) return rejected Promises — the application handles these
 * gracefully through its existing try/catch error-handling patterns (bootstrapSession,
 * loginOp.run, etc.).
 *
 * Suitable only for UI-level tests that do NOT require real Tauri IPC responses
 * (e.g., runtime_stability.spec.ts). Tests requiring real IPC must be skipped on
 * non-Windows platforms via test.skip(process.platform !== 'win32', ...).
 */
export class ViteDriver {
  private browser: ChromiumBrowser | null = null;
  private context: BrowserContext | null = null;

  public async start(): Promise<Page> {
    this.browser = await chromium.launch();
    this.context = await this.browser.newContext();

    // Inject __TAURI_INTERNALS__ shim BEFORE any page script executes.
    // @tauri-apps/api/core's invoke() reads window.__TAURI_INTERNALS__ synchronously
    // at call time. Without this shim it throws TypeError; with it, all calls return
    // a rejected Promise that the app catches internally.
    await this.context.addInitScript(() => {
      (window as any).__TAURI_INTERNALS__ = {
        invoke: (_cmd: string, _args?: unknown, _options?: unknown): Promise<unknown> => {
          // Return a sensible default for is_configured so the login page
          // renders correctly (false = unconfigured = show login form).
          // All other commands still reject so tests can verify graceful error handling.
          if (_cmd === 'is_configured') {
            return Promise.resolve(false);
          }
          return Promise.reject(new Error(`[ViteDriver] No Tauri backend available: ${_cmd}`));
        },

        // Minimal transformCallback shim required by some Tauri plugin internals
        transformCallback: (callback: Function, once?: boolean): number => {
          const id = Math.floor(Math.random() * 0x7fffffff);
          const key = `__tauriCb_${id}`;
          (window as any)[key] = once
            ? (...args: unknown[]) => {
                try {
                  callback(...args);
                } finally {
                  delete (window as any)[key];
                }
              }
            : callback;
          return id;
        },

        metadata: {
          currentWindow: { label: 'main' },
          windows: [{ label: 'main' }],
          config: {}
        },

        plugins: {}
      };
    });

    const page = await this.context.newPage();
    // Navigate to the Vite dev server (already started by Playwright's webServer config)
    await page.goto('http://localhost:1420', { waitUntil: 'domcontentloaded' });
    return page;
  }

  public async stop(): Promise<void> {
    if (this.context) {
      await this.context.close().catch(() => {});
      this.context = null;
    }
    if (this.browser) {
      await this.browser.close().catch(() => {});
      this.browser = null;
    }
  }

  /**
   * Not applicable in Vite dev server mode — no sandboxed SQLite database is created.
   * Returns a placeholder path to satisfy the driver interface.
   */
  public getDbPath(): string {
    return '/tmp/vite_driver_noop.db';
  }
}
