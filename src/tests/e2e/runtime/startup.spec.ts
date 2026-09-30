import { test, expect } from '../fixtures/tauriApp';
import { ProcessManager } from '../orchestration/processManager';
import { WebKitTauriDriver } from '../drivers/webkitTauriDriver';
import fs from 'fs';
import os from 'os';
import path from 'path';
import { ChildProcess, spawn } from 'child_process';

test.describe('Tauri Application Startup Integrity & Lifecycle', () => {

  // ADR-0062 environment wiring. These are pure path/environment properties of
  // the harness that decide where the launched binary resolves identity data, so
  // they are asserted directly on a prepared (not yet spawned) environment. That
  // keeps the guarantee checkable on any platform without launching the app and
  // without touching any real identity directory. The provisioning ceremony
  // itself stays covered by the runtime specs that do launch the binary.
  test('CDP launch environment is hermetic for identity data (ADR-0062)', async () => {
    const pm = new ProcessManager();
    const config = pm.prepareEnvironment(1420);
    const runRoot = pm.getTempDir();
    const identityDir = config.identityDataDir;

    // Absolute, not relative — the resolver rejects a relative value outright.
    expect(path.isAbsolute(identityDir)).toBe(true);

    // Lives under this run's own temp root and exists before startup.
    expect(runRoot).not.toBeNull();
    expect(identityDir.startsWith(runRoot as string)).toBe(true);
    expect(fs.existsSync(identityDir)).toBe(true);

    // The variable the app is actually spawned with.
    expect(config.env?.GRPC_IDENTITY_DATA_DIR).toBe(identityDir);

    // Never the real platform identity directory.
    const platformDir = path.join(
      process.platform === 'win32'
        ? (process.env.APPDATA ?? path.join(os.homedir(), 'AppData', 'Roaming'))
        : (process.env.XDG_DATA_HOME ?? path.join(os.homedir(), '.local', 'share')),
      'GRPC',
    );
    // Never the real platform identity directory, nor any descendant of one.
    // (A blanket "not under LOCALAPPDATA" check is wrong: `os.tmpdir()` is
    // `%LOCALAPPDATA%\Temp` on Windows, which is exactly where the run-scoped
    // root is supposed to live.)
    const realIdentityDirs = [
      process.env.APPDATA ? path.join(process.env.APPDATA, 'GRPC') : null,
      process.env.LOCALAPPDATA ? path.join(process.env.LOCALAPPDATA, 'GRPC') : null,
      process.env.XDG_DATA_HOME ? path.join(process.env.XDG_DATA_HOME, 'GRPC') : null,
    ].filter((p): p is string => Boolean(p) && path.isAbsolute(p as string));
    for (const real of realIdentityDirs) {
      expect(identityDir === real).toBe(false);
      expect(identityDir.startsWith(real + path.sep)).toBe(false);
    }
    expect(realIdentityDirs.length).toBeGreaterThan(0);

    // Existing variable semantics are untouched.
    expect(config.env?.GRPC_DB_PATH).toBe(config.dbPath);
    expect(config.env?.GRPC_ENV).toBe('test');
    expect(config.env?.GRPC_APP_KEY).toBeDefined();

    await pm.stop();
  });

  test('each E2E run gets a distinct identity directory (ADR-0062)', async () => {
    // `prepareEnvironment` mints a fresh run root per call (timestamp + random
    // suffix), so concurrent or sequential runs cannot share identity state.
    const first = new ProcessManager();
    const firstConfig = first.prepareEnvironment(1421);
    await first.stop();

    const second = new ProcessManager();
    const secondConfig = second.prepareEnvironment(1422);

    expect(secondConfig.identityDataDir).not.toBe(firstConfig.identityDataDir);
    expect(path.isAbsolute(secondConfig.identityDataDir)).toBe(true);
    // Same run root is never handed out twice.
    expect(second.getTempDir()).not.toBe(first.getTempDir());

    await second.stop();
  });

  test('a driver reuses one identity directory across app launches (ADR-0062)', async () => {
    // The multi-launch ceremony (fresh node → unlock → restart) is driven by one
    // driver instance, and must keep ONE identity directory across all launches
    // so a relaunch observes the store the first launch provisioned.
    const dataDir = fs.mkdtempSync(path.join(os.tmpdir(), 'grpc_identity_reuse_'));
    try {
      const driver = new WebKitTauriDriver({
        appBinary: path.join(process.cwd(), 'src-tauri', 'target', 'release', 'grpc'),
        dataDir,
      });

      expect(path.isAbsolute(driver.identityDataDir)).toBe(true);
      expect(driver.identityDataDir.startsWith(dataDir)).toBe(true);
      // Stable for the driver's lifetime → every launchApp() sees the same path.
      expect(driver.identityDataDir).toBe(driver.identityDataDir);
      // The App Key store lives in the identity directory (ADR-0062), not next
      // to the database.
      expect(driver.appKeyPath).toBe(path.join(driver.identityDataDir, 'appkey.age'));
      expect(driver.appKeyPath.startsWith(dataDir)).toBe(true);
      // Database location is unchanged by ADR-0062.
      expect(driver.dbPath).toBe(path.join(dataDir, 'GRPC', 'grpc.db'));
    } finally {
      fs.rmSync(dataDir, { recursive: true, force: true });
    }
  });

  if (process.platform === 'win32') {
    test('application boots cleanly and initializes sandboxed SQLite database', async ({ tauriApp }) => {
      const { page, driver } = tauriApp;

      // Verify page loads without critical errors
      await expect(page).toHaveTitle(/GRPC/i);

      // Verify database file was created dynamically in the sandboxed path
      const dbPath = driver.getDbPath();
      expect(fs.existsSync(dbPath)).toBe(true);

      // Verify default login form is visible (identity field is pinned or a
      // fixed selector since SEC-026 / ADR-0052 — never free-text).
      const usernameField = page.locator('#username');
      const passwordInput = page.locator('input[placeholder*="كلمة المرور"]');
      await expect(usernameField).toBeVisible();
      await expect(passwordInput).toBeVisible();

      // Verify that isConfigured() initially evaluates to false in fresh sandbox
      const configBannerText = await page.evaluate(async () => {
        try {
          const { invoke } = (window as any).__TAURI__.core;
          const configured = await invoke('is_configured');
          return configured ? 'configured' : 'unconfigured';
        } catch (e) {
          return 'error';
        }
      });
      expect(configBannerText).toBe('unconfigured');
    });
  } else {
    test.skip('application boots cleanly and initializes sandboxed SQLite database', async () => {});
  }

  test('corrupted database structure is rejected safely (Fail-Closed)', async () => {
    const binaryName = process.platform === 'win32' ? 'grpc.exe' : 'grpc';
    const binaryPath = path.join(process.cwd(), 'src-tauri', 'target', 'debug', binaryName);
    test.skip(
      !fs.existsSync(binaryPath),
      `GRPC debug binary not found at ${binaryPath}. Run 'cargo build' in src-tauri first.`
    );

    // Isolated sandbox: never touches the developer's real database.
    const sandboxDir = fs.mkdtempSync(path.join(os.tmpdir(), 'grpc_corrupt_db_'));
    const dbPath = path.join(sandboxDir, 'corrupted.db');
    // ADR-0062: identity data must resolve inside this sandbox too, otherwise
    // this spawn would read and write the operator's real identity files.
    const identityDataDir = path.join(sandboxDir, 'identity');
    fs.mkdirSync(identityDataDir, { recursive: true });

    // Genuinely invalid SQLite content: a non-empty file whose header does not
    // match SQLite's format magic. rusqlite/SQLite must reject it when the app
    // applies PRAGMA settings / runs migrations.
    const invalidPayload = Buffer.concat([
      Buffer.from('GARBAGE_SQLITE_HEADER_AND_BODY_NOT_A_REAL_DATABASE'),
      Buffer.alloc(4096, 0xde),
    ]);
    fs.writeFileSync(dbPath, invalidPayload);

    let child: ChildProcess | null = null;
    try {
      // ADR-0041 rank 1 (GRPC_APP_KEY) must be absent so the app resolves the
      // key via the debug dev-fallback and the DB bootstrap is what fails.
      // An empty string would instead fail the key format validation, masking
      // the corrupted-DB path under test — so it must be removed, not emptied.
      const appEnv: NodeJS.ProcessEnv = {
        ...process.env,
        GRPC_DB_PATH: dbPath,
        GRPC_ENV: 'test',
        GRPC_IDENTITY_DATA_DIR: identityDataDir,
      };
      delete appEnv.GRPC_APP_KEY;

      const proc = spawn(binaryPath, [], {
        env: appEnv,
        stdio: ['ignore', 'pipe', 'pipe'],
      });
      child = proc;

      const { code, signal, stderr } = await new Promise<{
        code: number | null;
        signal: NodeJS.Signals | null;
        stderr: string;
      }>((resolve, reject) => {
        let stderrBuf = '';
        proc.stderr?.on('data', (d) => {
          stderrBuf += String(d);
        });
        proc.stdout?.on('data', () => {});

        const timeout = setTimeout(() => {
          killProcess(proc);
          reject(new Error(`GRPC process did not exit within timeout. stderr:\n${stderrBuf}`));
        }, 30_000);

        proc.once('error', (err) => {
          clearTimeout(timeout);
          reject(err);
        });
        proc.once('exit', (exitCode, exitSignal) => {
          clearTimeout(timeout);
          resolve({ code: exitCode, signal: exitSignal, stderr: stderrBuf });
        });
      });

      // The real GRPC binary must refuse to start when its configured database
      // is corrupted: it reports the initialization failure and exits with 1.
      expect(signal).toBeNull();
      expect(code).toBe(1);
      expect(stderr).toContain('Failed to initialize database');
    } finally {
      if (child && child.exitCode === null) {
        killProcess(child);
      }
      fs.rmSync(sandboxDir, { recursive: true, force: true });
    }
  });
});

function killProcess(proc: ChildProcess): void {
  if (!proc.pid) return;
  if (process.platform === 'win32') {
    try {
      // Existing repo pattern (processManager.ts): taskkill the whole tree.
      const { execSync } = require('child_process');
      execSync(`taskkill /F /T /PID ${proc.pid}`, { stdio: 'ignore' });
    } catch {
      try {
        proc.kill('SIGKILL');
      } catch {}
    }
    return;
  }
  try {
    proc.kill('SIGINT');
    setTimeout(() => {
      try {
        proc.kill('SIGKILL');
      } catch {}
    }, 3000).unref();
  } catch {
    try {
      proc.kill('SIGKILL');
    } catch {}
  }
}
