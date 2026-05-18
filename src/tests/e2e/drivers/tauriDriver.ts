import { chromium } from '@playwright/test';
import type { ChromiumBrowser, Page } from '@playwright/test';
import { ProcessManager } from '../orchestration/processManager';
import { isPortInUse } from '../helpers/port';
import { startCdpRelay, type CdpRelayHandle } from '../helpers/cdpRelay';
import fs from 'fs';
import path from 'path';

export class TauriDriver {
  private processManager: ProcessManager;
  private browser: ChromiumBrowser | null = null;
  private relay: CdpRelayHandle | null = null;
  private port = 9222;

  constructor() {
    this.processManager = new ProcessManager();
  }

  /**
   * Automatically allocates unused ports, prepares the environment,
   * spawns the Tauri app, starts the Bun-native CDP relay, and attaches
   * Playwright to the WebView2 runtime through the relay.
   *
   * Background: Playwright's `connectOverCDP` uses the npm `ws` package
   * internally. Under Bun (no Node.js installed), `ws` cannot complete the
   * WebSocket handshake with WebView2. The relay uses Bun's native WebSocket
   * to proxy all CDP traffic so that Playwright never speaks directly to
   * WebView2's CDP socket.
   */
  public async start(): Promise<Page> {
    // ── 1. Allocate two consecutive free ports ────────────────────────────────
    //    cdpPort  : WebView2 remote-debugging port (passed to the app binary)
    //    relayPort: Bun relay port (Playwright talks to this one)
    let cdpPort = 9222;
    while (await isPortInUse(cdpPort) || await isPortInUse(cdpPort + 1)) {
      cdpPort++;
    }
    const relayPort = cdpPort + 1;
    this.port = cdpPort;

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
          `[cdpRelay] Timed out waiting for WebView2 CDP on port ${cdpPort}`
        );
      }
      await new Promise((resolve) => setTimeout(resolve, 500));
    }

    const logPath = path.join(path.dirname(this.getDbPath()), 'tauri_run.log');
    this._log(logPath, `[CDP READY] wsEndpoint=${wsEndpoint}`);

    // ── 4. Start the Bun-native relay ─────────────────────────────────────────
    //    The relay listens on relayPort and proxies all WebSocket + HTTP
    //    traffic to the real WebView2 CDP server on cdpPort using Bun's
    //    native WebSocket (which works correctly without Node.js).
    this.relay = startCdpRelay(cdpPort, relayPort);
    this._log(logPath, `[CDP RELAY] started: 127.0.0.1:${relayPort} → 127.0.0.1:${cdpPort}`);

    // Give the relay server a moment to bind before Playwright connects
    await new Promise((resolve) => setTimeout(resolve, 100));

    // ── 5. Connect Playwright through the relay ───────────────────────────────
    try {
      this.browser = await chromium.connectOverCDP(
        `http://127.0.0.1:${relayPort}`,
        { timeout: 30000 }
      );
      this._log(logPath, `[CDP CONNECTED] Browser instance created via relay`);
    } catch (err) {
      this._log(logPath, `[CDP CONNECT ERROR] ${err}`);
      throw err;
    }

    if (!this.browser) {
      throw new Error('[cdpRelay] CDP connection returned null browser');
    }

    // ── 6. Resolve the Tauri webview page ─────────────────────────────────────
    const contexts = this.browser.contexts();
    if (contexts.length === 0) {
      throw new Error('[cdpRelay] No browser contexts found after CDP connect');
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
      throw new Error('[cdpRelay] Tauri WebView page failed to initialize within time limit');
    }

    this._log(
      logPath,
      `[CDP ATTACH] Active context pages: ${JSON.stringify(pages.map((p) => p.url()))}`
    );

    return pages[0];
  }

  /**
   * Gracefully tears down the CDP connection, relay server, and app process.
   */
  public async stop(): Promise<void> {
    if (this.browser) {
      try {
        await this.browser.close();
      } catch {}
      this.browser = null;
    }
    if (this.relay) {
      this.relay.stop();
      this.relay = null;
    }
    await this.processManager.stop();
  }

  /**
   * Returns the sandboxed SQLite database file path for this instance.
   */
  public getDbPath(): string {
    return this.processManager.getDbPath();
  }

  private _log(logPath: string, message: string): void {
    try {
      fs.appendFileSync(logPath, `\n${message}\n`);
    } catch {}
  }
}
