// ADR-0041 operational acceptance — `/security` app-key lifecycle against the
// **Release** binary on Linux.
//
// Debug binaries are always "unlocked" (`cfg!(debug_assertions)`), so this spec
// requires: (1) a Release build, (2) the `tauri-driver` + `WebKitWebDriver`
// stack, and (3) an isolated `XDG_DATA_HOME`. It is skipped unless
// `E2E_RELEASE=1` and both binaries exist.
//
// Acceptance criteria:
//   fresh node:  setup → encrypted appkey.age → unlock → runtime bootstrap →
//                DB available → login
//   restart:     unlock existing store → same node identity → DB available
//   negative:    wrong passphrase / corrupted store / missing store / foreign
//                store all fail closed (no DB access while locked)

import { test, expect } from '@playwright/test';
import fs from 'fs';
import os from 'os';
import path from 'path';
import { WebKitTauriDriver } from '../drivers/webkitTauriDriver';

import { fileURLToPath } from 'url';
const __dirname = path.dirname(fileURLToPath(import.meta.url));

const RELEASE_BIN =
  process.env.GRPC_RELEASE_BIN ??
  path.resolve(__dirname, '..', '..', '..', '..', 'src-tauri', 'target', 'release', 'grpc');
const WEBKIT_DRIVER =
  process.env.GRPC_WEBKIT_DRIVER ??
  path.resolve(os.tmpdir(), 'opencode', 'webkitwebdriver', 'usr', 'bin', 'WebKitWebDriver');

const ENABLED =
  process.platform === 'linux' &&
  process.env.E2E_RELEASE === '1' &&
  fs.existsSync(RELEASE_BIN) &&
  fs.existsSync(WEBKIT_DRIVER);

const PASSPHRASE = 'grpc-e2e-passphrase-2026';
const WRONG_PASSPHRASE = 'definitely-not-the-passphrase';
const FOREIGN_PASSPHRASE = 'foreign-node-passphrase-99';

test.describe('ADR-0041 /security app-key lifecycle (Release binary)', () => {
  test.skip(!ENABLED, 'Requires E2E_RELEASE=1, a Release binary, and tauri-driver + WebKitWebDriver on Linux');

  function freshDataDir(): string {
    return fs.mkdtempSync(path.join(os.tmpdir(), 'grpc_sec_e2e_'));
  }

  function createDriver(dataDir: string): WebKitTauriDriver {
    return new WebKitTauriDriver({
      appBinary: RELEASE_BIN,
      webkitWebDriver: WEBKIT_DRIVER,
      dataDir,
    });
  }

  async function openSecurityPage(driver: WebKitTauriDriver): Promise<void> {
    await driver.client.execute(`window.location.hash = '/security'`);
    await driver.waitForHeading('أمان التطبيق');
  }

  test('fresh node: setup → encrypted store → unlock → bootstrap → DB → login', async () => {
    const dataDir = freshDataDir();
    const driver = createDriver(dataDir);
    try {
      await driver.launchApp();

      // Locked + unprovisioned: requires action, no DB bootstrap.
      const status0 = await driver.invoke<Record<string, unknown>>('get_security_status');
      expect(status0).toMatchObject({ provisioned: false, unlocked: false, requires_action: true });
      expect(status0.source).toBe('none');
      expect(fs.existsSync(driver.dbPath)).toBe(false);
      const lockedDb = await driver.tryInvoke('is_configured');
      expect(lockedDb.ok).toBe(false);

      // Setup form is shown (not the unlock form).
      await openSecurityPage(driver);
      expect(await driver.client.find('#security-passphrase')).not.toBeNull();
      expect(await driver.client.find('#security-unlock-passphrase')).toBeNull();

      // Create the app key through the UI.
      await driver.client.fill('#security-passphrase', PASSPHRASE);
      await driver.client.fill('#security-confirm-passphrase', PASSPHRASE);
      await driver.client.execute(
        `document.querySelector('#security-passphrase').closest('form').querySelector('button[type=submit]').click()`,
      );

      // Unlock success routes to the login page.
      await driver.client.waitFor('#username', 25000);

      // Backend proof: store persisted, DB bootstrapped, store-derived key.
      expect(fs.existsSync(driver.appKeyPath)).toBe(true);
      const storeFile = fs.readFileSync(driver.appKeyPath, 'utf8');
      const storeJson = JSON.parse(storeFile) as { format_version: number; encrypted_identity: number[] };
      expect(storeJson.format_version).toBe(1);
      expect(Array.isArray(storeJson.encrypted_identity)).toBe(true);
      const payload = Buffer.from(storeJson.encrypted_identity);
      expect(payload.subarray(0, 21).toString('latin1')).toBe('age-encryption.org/v1');
      expect(fs.existsSync(driver.dbPath)).toBe(true);
      const status1 = await driver.invoke<Record<string, unknown>>('get_security_status');
      expect(status1).toMatchObject({ provisioned: true, unlocked: true, requires_action: false });
      expect(status1.source).toBe('store');
      const configured = await driver.tryInvoke('is_configured');
      expect(configured.ok).toBe(true);
      expect(configured.value).toBe(false);

      // Login gate is reachable against the bootstrapped DB. Fresh nodes have
      // no seeded admin (B6-A: fleets bootstrap via the offline Root flow), so
      // `admin/admin` is rejected gracefully — proving the full auth IPC path
      // round-trips on the unlocked DB.
      await driver.client.fill('#username', 'admin');
      await driver.client.fill('#password', 'admin');
      await driver.client.execute(
        `document.querySelector('#username').closest('form').querySelector('button[type=submit]').click()`,
      );
      await driver.waitForBodyText('اسم المستخدم أو كلمة المرور غير صحيحة', 25000);
    } finally {
      await driver.stop();
    }
  });

  test('restart: unlock existing store → same node identity → DB available', async () => {
    const dataDir = freshDataDir();
    const driver = createDriver(dataDir);
    try {
      // Phase 1: fresh setup via IPC (UI setup is covered by the fresh-node test).
      await driver.launchApp();
      expect((await driver.invoke<Record<string, unknown>>('get_security_status')).requires_action).toBe(true);
      const init = await driver.invoke<Record<string, unknown>>('initialize_app_key', { passphrase: PASSPHRASE });
      expect(init.unlocked).toBe(true);
      expect(fs.existsSync(driver.dbPath)).toBe(true);
      const identityA = await driver.invoke<string>('export_app_key_backup');
      expect(identityA.startsWith('AGE-SECRET-KEY-1')).toBe(true);

      // Phase 2: shutdown + relaunch on the SAME data dir → locked again.
      await driver.quitApp();
      await driver.launchApp();
      const status = await driver.invoke<Record<string, unknown>>('get_security_status');
      expect(status).toMatchObject({ provisioned: true, unlocked: false, requires_action: true });
      // Locked restart: the store file exists (provisioned) but no key material
      // is live, so the source is 'none' until unlock.
      expect(status.source).toBe('none');
      const lockedDb = await driver.tryInvoke('is_configured');
      expect(lockedDb.ok).toBe(false);
      expect(fs.existsSync(driver.appKeyPath)).toBe(true);

      // Unlock form is shown after restart.
      await openSecurityPage(driver);
      expect(await driver.client.find('#security-unlock-passphrase')).not.toBeNull();
      expect(await driver.client.find('#security-passphrase')).toBeNull();
      await driver.client.fill('#security-unlock-passphrase', PASSPHRASE);
      await driver.client.execute(
        `document.querySelector('#security-unlock-passphrase').closest('form').querySelector('button[type=submit]').click()`,
      );
      await driver.client.waitFor('#username', 25000);

      // Same identity, DB available.
      const identityB = await driver.invoke<string>('export_app_key_backup');
      expect(identityB).toBe(identityA);
      expect((await driver.invoke<Record<string, unknown>>('get_security_status')).unlocked).toBe(true);
      const configured = await driver.tryInvoke('is_configured');
      expect(configured.ok).toBe(true);
    } finally {
      await driver.stop();
    }
  });

  test('negative: wrong passphrase stays locked, fail-closed', async () => {
    const dataDir = freshDataDir();
    const driver = createDriver(dataDir);
    try {
      await driver.launchApp();
      await driver.invoke('initialize_app_key', { passphrase: PASSPHRASE });
      await driver.quitApp();
      await driver.launchApp();

      const rejected = await driver.tryInvoke('unlock_app_key', { passphrase: WRONG_PASSPHRASE });
      expect(rejected.ok).toBe(false);

      const status = await driver.invoke<Record<string, unknown>>('get_security_status');
      expect(status).toMatchObject({ provisioned: true, unlocked: false, requires_action: true });
      expect((await driver.tryInvoke('is_configured')).ok).toBe(false);

      // UI surface shows the rejection.
      await openSecurityPage(driver);
      await driver.client.fill('#security-unlock-passphrase', WRONG_PASSPHRASE);
      await driver.client.execute(
        `document.querySelector('#security-unlock-passphrase').closest('form').querySelector('button[type=submit]').click()`,
      );
      await driver.client.waitFor('.bg-red-50, .text-red-700', 15000);
      const errorText = await driver.client.text('.bg-red-50, .text-red-700');
      expect(errorText).not.toHaveLength(0);
      expect((await driver.invoke<Record<string, unknown>>('get_security_status')).unlocked).toBe(false);

      // Correct passphrase still unlocks.
      await driver.client.fill('#security-unlock-passphrase', PASSPHRASE);
      await driver.client.execute(
        `document.querySelector('#security-unlock-passphrase').closest('form').querySelector('button[type=submit]').click()`,
      );
      await driver.client.waitFor('#username', 25000);
      expect((await driver.invoke<Record<string, unknown>>('get_security_status')).unlocked).toBe(true);
    } finally {
      await driver.stop();
    }
  });

  test('negative: corrupted appkey.age is rejected, fail-closed', async () => {
    const dataDir = freshDataDir();
    const driver = createDriver(dataDir);
    try {
      await driver.launchApp();
      await driver.invoke('initialize_app_key', { passphrase: PASSPHRASE });
      await driver.quitApp();

      fs.writeFileSync(driver.appKeyPath, 'CORRUPTED-AGE-STORE-NOT-VALID');

      await driver.launchApp();
      const rejected = await driver.tryInvoke('unlock_app_key', { passphrase: PASSPHRASE });
      expect(rejected.ok).toBe(false);

      const status = await driver.invoke<Record<string, unknown>>('get_security_status');
      expect(status).toMatchObject({ provisioned: true, unlocked: false, requires_action: true });
      expect((await driver.tryInvoke('is_configured')).ok).toBe(false);
    } finally {
      await driver.stop();
    }
  });

  test('negative: missing appkey requires setup and denies DB access', async () => {
    const dataDir = freshDataDir();
    const driver = createDriver(dataDir);
    try {
      await driver.launchApp();

      const rejected = await driver.tryInvoke('unlock_app_key', { passphrase: PASSPHRASE });
      expect(rejected.ok).toBe(false);
      // The frontend surfaces a localized generic error; the fail-closed
      // behaviour is what matters (the command rejected, no DB was touched).
      expect(typeof rejected.error).toBe('string');
      expect(rejected.error!.length).toBeGreaterThan(0);

      const status = await driver.invoke<Record<string, unknown>>('get_security_status');
      expect(status).toMatchObject({ provisioned: false, unlocked: false, requires_action: true });
      expect(status.source).toBe('none');
      expect(fs.existsSync(driver.dbPath)).toBe(false);
      expect((await driver.tryInvoke('is_configured')).ok).toBe(false);
    } finally {
      await driver.stop();
    }
  });

  test('negative: foreign store makes encrypted material unavailable', async () => {
    // Create a store owned by a different node/passphrase, then swap it in.
    const foreignDir = freshDataDir();
    const foreignDriver = createDriver(foreignDir);
    try {
      await foreignDriver.launchApp();
      await foreignDriver.invoke('initialize_app_key', { passphrase: FOREIGN_PASSPHRASE });
      await foreignDriver.quitApp();
    } finally {
      await foreignDriver.stop();
    }

    const dataDir = freshDataDir();
    fs.mkdirSync(path.join(dataDir, 'GRPC'), { recursive: true });
    fs.copyFileSync(path.join(foreignDir, 'GRPC', 'appkey.age'), path.join(dataDir, 'GRPC', 'appkey.age'));

    const driver = createDriver(dataDir);
    try {
      await driver.launchApp();
      const status = await driver.invoke<Record<string, unknown>>('get_security_status');
      expect(status).toMatchObject({ provisioned: true, unlocked: false, requires_action: true });

      // Original passphrase cannot decrypt the foreign store.
      const rejected = await driver.tryInvoke('unlock_app_key', { passphrase: PASSPHRASE });
      expect(rejected.ok).toBe(false);
      // Foreign passphrase is not available locally either — the material stays locked.
      expect((await driver.invoke<Record<string, unknown>>('get_security_status')).unlocked).toBe(false);
      expect((await driver.tryInvoke('is_configured')).ok).toBe(false);
      expect((await driver.tryInvoke('export_app_key_backup')).ok).toBe(false);
    } finally {
      await driver.stop();
    }
  });
});
