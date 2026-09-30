import { ChildProcess, spawn } from 'child_process';
import path from 'path';
import fs from 'fs';
import os from 'os';
import { waitForPort } from '../helpers/port';

export interface ProcessConfig {
  port: number;
  dbPath: string;
  /**
   * Absolute, run-scoped identity data directory (ADR-0062). Passed to the app
   * as `GRPC_IDENTITY_DATA_DIR` so identity state never resolves through the
   * operator's real platform data directory.
   */
  identityDataDir: string;
  env?: Record<string, string>;
}

export class ProcessManager {
  private process: ChildProcess | null = null;
  private tempDir: string | null = null;
  private config: ProcessConfig | null = null;
  private exitCode: number | null = null;
  private exitSignal: NodeJS.Signals | null = null;

  constructor() {}

  /**
   * Generates a isolated test environment with custom temp folders and DB paths.
   */
  public prepareEnvironment(port: number): ProcessConfig {
    const timestamp = Date.now();
    const randomSuffix = Math.floor(Math.random() * 100000);
    this.tempDir = path.join(os.tmpdir(), `grpc_e2e_${timestamp}_${randomSuffix}`);
    fs.mkdirSync(this.tempDir, { recursive: true });

    const dbPath = path.join(this.tempDir, 'test_sandbox.db');

    // ADR-0062: identity data directory resolved explicitly for the run. It
    // lives under this run's own temp root (`this.tempDir` is already unique per
    // run via timestamp + random suffix), so identity state is hermetic and
    // concurrent runs cannot collide. Created here rather than lazily so the
    // app never has to decide whether the directory exists.
    const identityDataDir = path.join(this.tempDir, 'identity');
    fs.mkdirSync(identityDataDir, { recursive: true });

    // Default development AGE-X25519 identity key
    const testAppKey = 'AGE-SECRET-KEY-1KTYK6RVLN5TAPE7VF6FQQSKZ9HWWCDSKUGXXNUQDWZ7XXT5YK5LSF3UTKQ';

    this.config = {
      port,
      dbPath,
      identityDataDir,
      env: {
        GRPC_DB_PATH: dbPath,
        GRPC_ENV: 'test',
        GRPC_APP_KEY: testAppKey,
        // ADR-0062: must be set before the app resolves identity data. The
        // platform resolver does not honor XDG_DATA_HOME on Windows, so this
        // is the only reliable isolation for the CDP path.
        GRPC_IDENTITY_DATA_DIR: identityDataDir,
        WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`,
        WEBVIEW2_USER_DATA_FOLDER: path.join(this.tempDir, 'webview2'),
        TAURI_ENV_DEBUG: 'true'
      }
    };

    return this.config;
  }

  /**
   * Spawns the Tauri binary process under debug mode.
   */
  public async start(): Promise<ChildProcess> {
    if (!this.config) {
      throw new Error('Call prepareEnvironment() before starting process');
    }

    const binaryName = process.platform === 'win32' ? 'grpc.exe' : 'grpc';
    const exePath = path.join(process.cwd(), 'src-tauri/target/debug', binaryName);
    if (!fs.existsSync(exePath)) {
      throw new Error(`Tauri binary not found at: ${exePath}. Run 'cargo build' inside 'src-tauri' first.`);
    }

    // Spawn process
    this.process = spawn(exePath, [], {
      env: {
        ...process.env,
        ...this.config.env,
      },
      stdio: 'pipe'
    });

    const logPath = this.getRunLogPath()!;
    const logStream = fs.createWriteStream(logPath, { flags: 'a' });
    this.process.stdout?.pipe(logStream);
    this.process.stderr?.pipe(logStream);

    this.process.on('exit', (code, signal) => {
      this.exitCode = code;
      this.exitSignal = signal;
      try {
        fs.appendFileSync(logPath, `\n[PROCESS EXITED] code=${code} signal=${signal}\n`);
      } catch (e) {}
      if (code !== null && code !== 0) {
        console.error(`[Tauri Process Exit] Tauri process exited prematurely with code ${code}. Logs: ${logPath}`);
        try {
          const logs = fs.readFileSync(logPath, 'utf8');
          console.error(`--- TAURI RUN LOG START ---\n${logs}\n--- TAURI RUN LOG END ---`);
        } catch (e) {
          console.error(`Could not read log file: ${e}`);
        }
      }
    });

    this.process.on('error', (err) => {
      try {
        fs.appendFileSync(logPath, `\n[PROCESS ERROR] ${err}\n`);
      } catch (e) {}
      console.error(`[Tauri Process Error] Spawn failed: ${err}`);
    });

    try {
      await waitForPort(this.config.port);
    } catch (err) {
      throw new Error(
        `[TauriDriver] ${(err as Error).message}\n${this.getDiagnostics()}`
      );
    }
    return this.process;
  }

  /**
   * Returns the path of the spawned process stdout/stderr capture file.
   * Callers must read it before stop() deletes the temporary directory.
   */
  public getRunLogPath(): string | null {
    if (!this.tempDir) {
      return null;
    }
    return path.join(this.tempDir, 'tauri_run.log');
  }

  /**
   * Returns a human-readable summary of the spawned process lifecycle state,
   * including the tail of the run log when the process wrote one.
   */
  public getDiagnostics(): string {
    const port = this.config?.port ?? 'unknown';
    const pid = this.process?.pid ?? 'none';
    const exited = this.exitCode !== null || this.exitSignal !== null;
    const exitInfo = exited
      ? `code=${this.exitCode} signal=${this.exitSignal}`
      : 'still running (never reported an exit)';
    const lines = [
      `port=${port}`,
      `pid=${pid}`,
      `state=${exitInfo}`,
      `binary=${path.join(process.cwd(), 'src-tauri/target/debug', process.platform === 'win32' ? 'grpc.exe' : 'grpc')}`,
    ];
    try {
      const logPath = this.getRunLogPath();
      if (logPath && fs.existsSync(logPath)) {
        const tail = fs.readFileSync(logPath, 'utf8').trimEnd().split('\n').slice(-40);
        lines.push(`run log (${logPath}, last ${tail.length} lines):`, ...tail);
      } else {
        lines.push('run log: missing or empty (process produced no output)');
      }
    } catch (e) {
      lines.push(`run log: unreadable (${e})`);
    }
    return lines.join('\n');
  }

  /**
   * Cleans up the process and temporary directory.
   */
  public async stop(): Promise<void> {
    if (this.process && this.process.pid) {
      const pid = this.process.pid;
      if (process.platform === 'win32') {
        try {
          const { execSync } = require('child_process');
          execSync(`taskkill /F /T /PID ${pid}`, { stdio: 'ignore' });
        } catch (e) {
          // Fallback if taskkill fails
          this.process.kill('SIGKILL');
        }
      } else {
        this.process.kill('SIGINT');
        const p = this.process;
        await new Promise<void>((resolve) => {
          const timeout = setTimeout(() => {
            p.kill('SIGKILL');
            resolve();
          }, 3000);

          p.on('exit', () => {
            clearTimeout(timeout);
            resolve();
          });
        });
      }
      this.process = null;
    }

    // Clean up temporary files with retries (Windows file locks can occasionally delay deletion)
    if (this.tempDir && fs.existsSync(this.tempDir)) {
      for (let attempt = 1; attempt <= 3; attempt++) {
        try {
          fs.rmSync(this.tempDir, { recursive: true, force: true });
          break;
        } catch (e) {
          if (attempt === 3) {
            console.warn(`Could not completely clean up temp dir: ${this.tempDir}`);
          } else {
            await new Promise((resolve) => setTimeout(resolve, 500));
          }
        }
      }
      this.tempDir = null;
    }
  }

  public getDbPath(): string {
    if (!this.config) {
      throw new Error('Environment not prepared');
    }
    return this.config.dbPath;
  }

  /**
   * Returns the absolute, run-scoped identity data directory handed to the app
   * as `GRPC_IDENTITY_DATA_DIR` (ADR-0062).
   */
  public getIdentityDataDir(): string {
    if (!this.config) {
      throw new Error('Environment not prepared');
    }
    return this.config.identityDataDir;
  }

  /**
   * Returns the run-scoped temp root that owns both the database and the
   * identity directory. Callers use it to assert hermetic isolation.
   */
  public getTempDir(): string | null {
    return this.tempDir;
  }
}
