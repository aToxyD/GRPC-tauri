// Linux WebKit Tauri driver for Release-binary E2E (ADR-0041 operational
// acceptance). Orchestrates `tauri-driver` + the Debian `WebKitWebDriver`
// binary so the real Release binary can be driven over the W3C WebDriver
// protocol on Linux. Debug builds are always "unlocked"
// (`cfg!(debug_assertions)` in `src-tauri/src/infrastructure/security/mod.rs`),
// so the `/security` flow can only be exercised against a Release binary with
// an isolated `XDG_DATA_HOME` and **no** `GRPC_APP_KEY`.

import { ChildProcess, spawn } from 'child_process';
import fs from 'fs';
import net from 'net';
import os from 'os';
import path from 'path';
import { waitForPort } from '../helpers/port';
import { WebDriverClient, WebDriverError } from './webDriverClient';

export interface WebKitTauriDriverOptions {
  /** Release binary of the app (e.g. src-tauri/target/release/grpc). */
  appBinary: string;
  /** Path to the `tauri-driver` executable. */
  tauriDriverBinary?: string;
  /** Path to the `WebKitWebDriver` binary (Linux). Default: found on PATH. */
  webkitWebDriver?: string;
  /** Sandboxed `XDG_DATA_HOME` for the launched app (isolates DB + logs). */
  dataDir: string;
  /**
   * Absolute, run-scoped identity data directory (ADR-0062). Defaults to
   * `<dataDir>/GRPC` — the same directory `XDG_DATA_HOME` already resolved to,
   * so existing `security.spec.ts` assertions on `appkeyPath` keep holding.
   */
  identityDataDir?: string;
  /** Extra environment for the launched app (merged over the defaults). */
  env?: Record<string, string>;
  /** tauri-driver intermediary port (default: auto-assigned). */
  port?: number;
}

interface SpawnedSession {
  driver: ChildProcess;
  client: WebDriverClient;
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function findFreePort(): Promise<number> {
  return new Promise((resolve, reject) => {
    const srv = net.createServer();
    srv.unref();
    srv.on('error', reject);
    srv.listen(0, '127.0.0.1', () => {
      const address = srv.address() as net.AddressInfo;
      const port = address.port;
      srv.close(() => resolve(port));
    });
  });
}

export class WebKitTauriDriver {
  private readonly appBinary: string;
  private readonly tauriDriverBinary: string;
  private readonly webkitWebDriver?: string;
  readonly dataDir: string;
  /**
   * Absolute identity data directory handed to the app as
   * `GRPC_IDENTITY_DATA_DIR` (ADR-0062). Fixed for the lifetime of the driver,
   * so a restart within one test reuses the same identity state.
   */
  readonly identityDataDir: string;
  private readonly extraEnv: Record<string, string>;
  private spawned: SpawnedSession | null = null;
  private readonly logs: string[] = [];

  constructor(options: WebKitTauriDriverOptions) {
    this.appBinary = options.appBinary;
    this.tauriDriverBinary =
      options.tauriDriverBinary ??
      path.join(os.homedir(), '.cargo', 'bin', 'tauri-driver');
    this.webkitWebDriver = options.webkitWebDriver;
    this.dataDir = options.dataDir;
    // Defaults to the same `<dataDir>/GRPC` directory the app already resolved
    // through `XDG_DATA_HOME` on Linux, so existing assertions in
    // `security.spec.ts` (which locate `appkey.age` under that path) keep
    // holding. The difference is that it is now stated explicitly rather than
    // inferred from `XDG_DATA_HOME`, which the platform resolver does not honor
    // on Windows. Specs may override to place identity apart from the database.
    this.identityDataDir = options.identityDataDir ?? path.join(options.dataDir, 'GRPC');
    this.extraEnv = options.env ?? {};
  }

  get client(): WebDriverClient {
    if (!this.spawned) throw new Error('Driver not started');
    return this.spawned.client;
  }

  get dbPath(): string {
    return path.join(this.dataDir, 'GRPC', 'grpc.db');
  }

  /**
   * Location of the encrypted App Key store, which lives in the ADR-0062
   * identity directory rather than next to the database.
   */
  get appKeyPath(): string {
    return path.join(this.identityDataDir, 'appkey.age');
  }

  /**
   * Start `tauri-driver` and wait for its WebDriver endpoint. The app
   * environment is baked into tauri-driver so it is inherited by
   * `WebKitWebDriver` and then by the app it launches.
   */
  async start(): Promise<void> {
    if (this.spawned) return;

    if (!fs.existsSync(this.tauriDriverBinary)) {
      throw new Error(
        `tauri-driver not found at ${this.tauriDriverBinary}. Install with: cargo install tauri-driver`,
      );
    }
    if (!fs.existsSync(this.appBinary)) {
      throw new Error(`Release binary not found at ${this.appBinary}. Run 'bun run tauri:build' first.`);
    }
    fs.mkdirSync(path.join(this.dataDir, 'GRPC'), { recursive: true });
    // ADR-0062: the identity directory is created up front so the app never
    // has to create it, and never falls back to any other location.
    fs.mkdirSync(this.identityDataDir, { recursive: true });

    const port = await findFreePort();
    const nativePort = await findFreePort();

    // The app must inherit: isolated data dir, non-production env, and NO
    // GRPC_APP_KEY (ADR-0041 rank 1 must be absent so the store flow runs).
    //
    // ADR-0062: `GRPC_IDENTITY_DATA_DIR` is set explicitly rather than relying
    // on `XDG_DATA_HOME`. The platform resolver does not honor `XDG_DATA_HOME`
    // on Windows, so identity data would otherwise resolve through the real
    // per-user data directory on that platform.
    const appEnv: NodeJS.ProcessEnv = {
      ...process.env,
      XDG_DATA_HOME: this.dataDir,
      GRPC_IDENTITY_DATA_DIR: this.identityDataDir,
      GRPC_ENV: 'test',
      WEBKIT_DISABLE_COMPOSITING_MODE: '1',
      ...this.extraEnv,
    };
    delete appEnv.GRPC_APP_KEY;
    delete appEnv.GRPC_DB_PATH;

    const args: string[] = [
      '--port',
      String(port),
      '--native-port',
      String(nativePort),
      '--native-host',
      '127.0.0.1',
    ];
    if (this.webkitWebDriver) {
      args.push('--native-driver', this.webkitWebDriver);
    }

    const driver = spawn(this.tauriDriverBinary, args, {
      env: appEnv,
      stdio: ['ignore', 'pipe', 'pipe'],
      // Own process group so the launched app (grandchild) can be killed
      // together with the driver. WebKitWebDriver does NOT terminate the app
      // on session delete; without this, orphaned single-instance apps block
      // every subsequent launch.
      detached: true,
    });
    this.spawned = { driver, client: new WebDriverClient(port) };

    driver.stdout?.on('data', (d) => this.logs.push(String(d)));
    driver.stderr?.on('data', (d) => this.logs.push(String(d)));

    try {
      await waitForPort(port, 30000);
    } catch (err) {
      await this.stop();
      throw new Error(
        `tauri-driver did not come up on port ${port}. Logs:\n${this.logs.join('')}`,
      );
    }
  }

  /**
   * Launch the app (fresh session). Each session is a fresh app process; the
   * same data dir can be reused across sessions to simulate restarts.
   */
  async launchApp(): Promise<void> {
    await this.start();
    let lastErr: unknown = null;
    for (let attempt = 0; attempt < 4; attempt++) {
      try {
        await this.client.createSession(this.appBinary);
        await this.waitForReady();
        return;
      } catch (err) {
        lastErr = err;
        if (this.spawned?.client.sessionId) {
          try {
            await this.client.deleteSession();
          } catch {
            // ignore
          }
        }
        await sleep(1500);
      }
    }
    const detail = lastErr instanceof WebDriverError ? lastErr.message : String(lastErr);
    throw new Error(
      `Failed to create WebDriver session (app launch failed): ${detail}\nDriver logs:\n${this.logs.join('')}`,
    );
  }

  /**
   * Wait until the webview has loaded and the Tauri IPC bridge is reachable.
   */
  async waitForReady(timeoutMs = 30000): Promise<void> {
    const start = Date.now();
    for (;;) {
      try {
        const ready = await this.client.execute<boolean>(
          `document.readyState === 'complete' && !!window.__TAURI__?.core?.invoke`,
        );
        if (ready) return;
      } catch {
        // driver/session not ready yet
      }
      if (Date.now() - start > timeoutMs) {
        throw new Error('Timeout waiting for webview to become ready');
      }
      await sleep(300);
    }
  }

  /** End the session and kill the app. WebKitWebDriver does not terminate the
   *  app on session delete, so the whole process group (tauri-driver →
   *  WebKitWebDriver → app) is SIGKILLed; the single-instance lock is thereby
   *  released for the next launch. */
  async quitApp(): Promise<void> {
    if (this.spawned?.client.sessionId) {
      try {
        await this.client.deleteSession();
      } catch {
        // session already gone
      }
    }
    this.killProcessGroup();
    // Give the app time to release its single-instance lock before relaunch.
    await sleep(1200);
  }

  async stop(): Promise<void> {
    if (this.spawned?.client.sessionId) {
      try {
        await this.client.deleteSession();
      } catch {
        // session already gone
      }
    }
    this.killProcessGroup();
  }

  private killProcessGroup(): void {
    const proc = this.spawned?.driver;
    this.spawned = null;
    if (!proc?.pid) return;
    try {
      process.kill(-proc.pid, 'SIGKILL');
    } catch {
      try {
        proc.kill('SIGKILL');
      } catch {
        // already dead
      }
    }
  }

  // ---- convenience wrappers ----

  async invoke<T = unknown>(command: string, args?: Record<string, unknown>): Promise<T> {
    const argJson = JSON.stringify(args ?? {});
    const script = `(async () => {
      try {
        return { _value: await window.__TAURI__.core.invoke(${JSON.stringify(command)}, ${argJson}) };
      } catch (e) {
        return { _error: e && e.message !== undefined ? String(e.message) : String(e) };
      }
    })()`;
    const res = await this.client.execute<{ _value?: T; _error?: string }>(script);
    if (res && typeof res === 'object' && '_error' in res) {
      throw new Error(res._error);
    }
    return (res as { _value?: T } | null)?._value as T;
  }

  async tryInvoke(command: string, args?: Record<string, unknown>): Promise<{ ok: boolean; value?: unknown; error?: string }> {
    try {
      const value = await this.invoke(command, args);
      return { ok: true, value };
    } catch (err) {
      return { ok: false, error: err instanceof Error ? err.message : String(err) };
    }
  }

  async waitForHeading(text: string, timeoutMs = 20000): Promise<void> {
    const start = Date.now();
    for (;;) {
      try {
        const titles = await this.client.execute<string[]>(
          `Array.from(document.querySelectorAll('h1,h2')).map((e) => e.textContent || '')`,
        );
        if (titles.some((t) => t.includes(text))) return;
      } catch {
        // driver not ready yet
      }
      if (Date.now() - start > timeoutMs) {
        throw new Error(`Timeout waiting for heading containing "${text}"`);
      }
      await sleep(300);
    }
  }

  async waitForBodyText(text: string, timeoutMs = 20000): Promise<void> {
    const start = Date.now();
    for (;;) {
      try {
        const body = await this.client.execute<string>(
          `document.body ? document.body.innerText : ''`,
        );
        if (body.includes(text)) return;
      } catch {
        // driver not ready yet
      }
      if (Date.now() - start > timeoutMs) {
        throw new Error(`Timeout waiting for body text containing "${text}"`);
      }
      await sleep(300);
    }
  }
}
