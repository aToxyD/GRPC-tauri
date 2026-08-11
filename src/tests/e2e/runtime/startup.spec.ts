import { test, expect } from '../fixtures/tauriApp';
import fs from 'fs';
import os from 'os';
import path from 'path';
import { ChildProcess, spawn } from 'child_process';

test.describe('Tauri Application Startup Integrity & Lifecycle', () => {

  if (process.platform === 'win32') {
    test('application boots cleanly and initializes sandboxed SQLite database', async ({ tauriApp }) => {
      const { page, driver } = tauriApp;

      // Verify page loads without critical errors
      await expect(page).toHaveTitle(/GRPC/i);

      // Verify database file was created dynamically in the sandboxed path
      const dbPath = driver.getDbPath();
      expect(fs.existsSync(dbPath)).toBe(true);

      // Verify default login form is visible
      const usernameInput = page.locator('input[placeholder*="اسم المستخدم"]');
      const passwordInput = page.locator('input[placeholder*="كلمة المرور"]');
      await expect(usernameInput).toBeVisible();
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
      const appEnv: NodeJS.ProcessEnv = { ...process.env, GRPC_DB_PATH: dbPath, GRPC_ENV: 'test' };
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
