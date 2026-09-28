import { chromium } from '@playwright/test';
import type { ChromiumBrowser, Page } from '@playwright/test';
import { ProcessManager } from '../orchestration/processManager';
import { resolveCdpPort } from '../helpers/cdpPort';
import fs from 'fs';
import path from 'path';

export class TauriDriver {
  private processManager: ProcessManager;
  private browser: ChromiumBrowser | null = null;
  private port = resolveCdpPort();

  constructor() {
    this.processManager = new ProcessManager();
  }

  /**
   * Resolves the fixed CDP port, prepares the environment, spawns the Tauri
   * app, and attaches Playwright directly to the WebView2 runtime.
   *
   * The port is NOT dynamically allocated: it is compiled into the app binary
   * via the window's `additionalBrowserArgs`, so it must match
   * `scripts/e2e/tauri_cdp_overlay.ts`. Both read `helpers/cdpPort.ts`.
   *
   * The endpoint is used exactly as WebView2 reports it. Step 3 already
   * resolved `webSocketDebuggerUrl` from `/json/version`, and that is the
   * unmodified WebSocket endpoint of the running WebView2 instance, so
   * `connectOverCDP` is handed the real CDP socket with no rewriting and no
   * intermediate proxy. Passing the `ws://` URL also avoids a second
   * `/json/version/` round trip, which Playwright only issues when it is given
   * an `http://` endpoint.
   *
   * The `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS` environment variable set below
   * is intentionally retained as a fallback for WebView2 builds that do not
   * consume the compiled window argument, but it is NOT sufficient on its own:
   * wry always calls `set_additional_browser_arguments(...)` with a non-empty
   * value, which overrides that environment variable entirely.
   */
  public async start(): Promise<Page> {
    // ── 1. Resolve the build-time CDP port ────────────────────────────────────
    const cdpPort = this.port;

    // ── 2. Prepare environment & launch app ───────────────────────────────────
    this.processManager.prepareEnvironment(cdpPort);
    await this.processManager.start();

    // ── 3. Poll the real CDP HTTP endpoint until WebView2 is ready ───────────
    const versionUrl = `http://127.0.0.1:${cdpPort}/json/version`;
    let wsEndpoint = '';
    let retries = 30;
    while (retries > 0) {
      try {
        const response = await fetch(versionUrl);
        if (response.ok) {
          const data = (await response.json()) as Record<string, string>;
          wsEndpoint = data.webSocketDebuggerUrl ?? '';
          if (wsEndpoint) break;
        }
      } catch {}
      retries--;
      if (retries === 0) {
        throw new Error(
          `[tauriDriver] Timed out waiting for WebView2 CDP on port ${cdpPort}`
        );
      }
      await new Promise((resolve) => setTimeout(resolve, 500));
    }

    const logPath = path.join(path.dirname(this.getDbPath()), 'tauri_run.log');
    this._log(logPath, `[CDP READY] wsEndpoint=${wsEndpoint}`);

    // ── 4. Connect Playwright directly to the WebView2 CDP endpoint ───────────
    try {
      this.browser = await chromium.connectOverCDP(wsEndpoint, { timeout: 30000 });
      this._log(logPath, `[CDP CONNECTED] Browser instance created on ${wsEndpoint}`);
    } catch (err) {
      this._log(logPath, `[CDP CONNECT ERROR] ${err}`);
      throw err;
    }

    if (!this.browser) {
      throw new Error('[tauriDriver] CDP connection returned null browser');
    }

    // ── 5. Resolve the Tauri webview page ─────────────────────────────────────
    const contexts = this.browser.contexts();
    if (contexts.length === 0) {
      throw new Error('[tauriDriver] No browser contexts found after CDP connect');
    }

    const context = contexts[0];
    let pages = context.pages();

    let attempts = 30;
    while (pages.length === 0 && attempts > 0) {
      await new Promise((resolve) => setTimeout(resolve, 100));
      pages = context.pages();
      attempts--;
    }

    if (pages.length === 0) {
      throw new Error('[tauriDriver] Tauri WebView page failed to initialize within time limit');
    }

    this._log(
      logPath,
      `[CDP ATTACH] Active context pages: ${JSON.stringify(pages.map((p) => p.url()))}`
    );

    return pages[0];
  }

  /**
   * Gracefully tears down the CDP connection and the app process.
   */
  public async stop(): Promise<void> {
    if (this.browser) {
      try {
        await this.browser.close();
      } catch {}
      this.browser = null;
    }
    await this.processManager.stop();
  }

  /**
   * Returns the sandboxed SQLite database file path for this instance.
   */
  public getDbPath(): string {
    return this.processManager.getDbPath();
  }

  /**
   * Returns the path of the app stdout/stderr capture file, or null when the
   * environment was never prepared. Callers must read it before stop().
   */
  public getRunLogPath(): string | null {
    return this.processManager.getRunLogPath();
  }

  /**
   * Returns the spawned process lifecycle state plus the tail of its run log.
   */
  public getDiagnostics(): string {
    return this.processManager.getDiagnostics();
  }

  private _log(logPath: string, message: string): void {
    try {
      fs.appendFileSync(logPath, `\n${message}\n`);
    } catch {}
  }
}
