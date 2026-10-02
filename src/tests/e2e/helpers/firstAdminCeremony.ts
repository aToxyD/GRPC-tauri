/**
 * First-ADMIN bootstrap ceremony for the Tauri E2E suite.
 *
 * Production does NOT seed a password-enabled `admin` (B6-A): a fresh node has
 * no user row at all, and `IdentityAuthenticationPolicy::password_login_allowed`
 * returns `true` for a missing row (anti-enumeration), so a password login fails
 * generically and never redirects to the admin-key path. The only way to reach
 * an authenticated state is the real provisioning ceremony, which this helper
 * drives end to end:
 *
 *   1. `root-signer init`            — isolated Authority Root keypair
 *   2. `begin_wilaya_provision`       — node key + unsigned CSR (pre-auth)
 *   3. `root-signer sign`            — offline Root signature, external process
 *   4. `finalize_wilaya_provision`   — Root-verified ACTIVE WILAYA (pre-auth)
 *   5. `issue_first_admin_key`       — identity-only ADMIN + `.adminkey` (pre-auth)
 *   6. admin-key UI login            — real `handleChallengeLogin` → `afterLogin`
 *   7. WILAYA configuration form     — `configure_as_wilaya` (AdminOnly session)
 *   8. secure-session + ADR-0062 isolation assertions
 *
 * Nothing here bypasses, relaxes, or seeds production authentication: every step
 * invokes an existing pre-auth/post-auth command exactly as the operator does.
 *
 * Trust note: the Root keypair is generated per run and ONLY its public half
 * reaches the application (`GRPC_ROOT_PUBLIC_KEY`). The private half never
 * enters the application sandbox and never enters an environment variable; it is
 * passed to `root-signer` via `--key-file` so it also stays out of the child
 * process command line.
 *
 * @module e2e/helpers/firstAdminCeremony
 */
import { spawnSync } from 'child_process';
import { existsSync, mkdtempSync, readFileSync, rmSync } from 'fs';
import os from 'os';
import path from 'path';
import { expect, type Page } from '@playwright/test';
import type { TauriDriver } from '../drivers/tauriDriver';
import { RuntimeContracts } from '../contracts/runtimeContracts';
import type {
  FinalizeWilayaProvisionResultDto,
  IdentityBootstrapState,
  IdentityCertificateDto,
} from '../../../lib/contracts/identity.contract';

// ── Repository conventions ────────────────────────────────────────────────────

/**
 * `cargo build` inside `src-tauri` emits every `[[bin]]` into `target/debug`,
 * which is the same location `ProcessManager` resolves the app binary from.
 * `root-signer` is declared in `src-tauri/Cargo.toml` alongside `grpc`.
 */
const ROOT_SIGNER_BIN_DIR = 'src-tauri/target/debug';

/** Root private key file produced by `root-signer init`. Never leaves the ceremony dir. */
const ROOT_SECRET_FILE = 'root-secret.hex';
/** Root public key file. This is the only artifact that leaves the ceremony dir. */
const ROOT_PUBLIC_KEY_FILE = 'root-public.key';

/** CSR emitted by `begin_wilaya_provision`. Must end in `.json` (`validate_file_path`). */
const CSR_FILE = 'wilaya-csr.json';
/** Root-signed certificate consumed by `finalize_wilaya_provision`. */
const SIGNED_CERT_FILE = 'wilaya-signed.json';

/**
 * Identity artifacts governed by ADR-0062. Filenames mirror the Rust constants
 * `NODE_KEY_FILE_NAME`, `NODE_KEY_PENDING_FILE_NAME`, `ADMINKEY_FILE_NAME`, and
 * `APPKEY_FILE_NAME`.
 */
const NODE_KEY_FILE = 'node_identity.key';
const NODE_KEY_PENDING_FILE = 'node_identity.key.pending';
const ADMINKEY_FILE = '.adminkey';
const APPKEY_FILE = 'appkey.age';
const IDENTITY_ARTIFACTS = [
  NODE_KEY_FILE,
  NODE_KEY_PENDING_FILE,
  ADMINKEY_FILE,
  APPKEY_FILE,
];

/** `GRPC_DATA_DIR` — the platform default identity location on Windows is `%APPDATA%\GRPC`. */
const GRPC_PLATFORM_DIR_NAME = 'GRPC';

/** `ED25519_PUBLIC_KEY_LEN` — a Base64 Ed25519 public key decodes to exactly 32 bytes. */
const ED25519_PUBLIC_KEY_LEN = 32;

/** `BOOTSTRAP_ADMIN_USERNAME` — the canonical operator username is pinned (ADR-0052). */
export const E2E_ADMIN_USERNAME = 'admin';

/**
 * E2E-only passphrase protecting the throwaway `.adminkey`. This is a
 * well-known non-secret placeholder, matching the `ADMIN_PASSPHRASE`
 * convention already used by `src-tauri/tests/root_signer_e2e.rs`. It is not a
 * production credential, is never printed, and only ever encrypts a per-run
 * `.adminkey` that is destroyed with the sandbox.
 */
export const E2E_ADMIN_PASSPHRASE = 'correct horse battery staple';

const E2E_WILAYA_CODE = '16';
const E2E_WILAYA_NAME = 'Alger';

// ── UI selectors (current LoginPage / WilayaNodeSetupPage contract) ───────────

/** LoginPage renders two `role="tab"` buttons; index 1 is the admin-key tab. */
const AUTH_TAB_SELECTOR = '[role="tab"]';
const ADMIN_KEY_TAB_INDEX = 1;
/** Admin-key passphrase field. */
const PASSPHRASE_INPUT = '#passphrase';
/**
 * The two login forms are mutually exclusive (`{#if authTab === ...}`), so once
 * the admin-key tab is active this resolves to the admin-key submit button and
 * never the password form.
 */
const ADMIN_KEY_SUBMIT = 'form button[type="submit"]';
const WILAYA_CODE_INPUT = 'input[id="wilayaCode"]';
const WILAYA_NAME_INPUT = 'input[id="wilayaName"]';
/**
 * NOTE: the configure page heading and its submit button differ by one word
 * (`تكوين الولاية` vs `تكوين كولاية`). They must not be conflated.
 */
const CONFIGURE_HEADING = 'h1:has-text("تكوين الولاية")';
const CONFIGURE_SUBMIT = 'button:has-text("تكوين كولاية")';
const DASHBOARD_HEADING = 'h1:has-text("لوحة تحكم الولاية")';

/**
 * Generous per-step budget. The admin-key tab only becomes enabled after a page
 * reload re-observes the identity state, and the configure form navigates on a
 * 1500 ms timer, so the shared 5 s expect timeout is too tight here.
 */
const CEREMONY_STEP_TIMEOUT = 30_000;

// ── Root ceremony ─────────────────────────────────────────────────────────────

/**
 * Handle to one isolated Authority Root ceremony. Owns its own directory because
 * `ProcessManager` only cleans up the application sandbox — the Root private key
 * must be removed explicitly.
 */
export interface RootCeremony {
  /** Isolated ceremony directory, deliberately outside the application sandbox. */
  readonly dir: string;
  /** Trimmed Base64 Ed25519 Root public key — the only value given to the app. */
  readonly publicKey: string;
  /** Absolute path of the Root private key; passed only to `root-signer --key-file`. */
  readonly privateKeyFile: string;
  /** Removes the ceremony directory. Must be called during fixture teardown. */
  cleanup(): void;
}

/**
 * Resolves the `root-signer` executable using the same convention as
 * `ProcessManager.start()` rather than inventing a path.
 */
function resolveRootSignerBinary(): string {
  const binaryName = process.platform === 'win32' ? 'root-signer.exe' : 'root-signer';
  const exePath = path.join(process.cwd(), ROOT_SIGNER_BIN_DIR, binaryName);
  if (!existsSync(exePath)) {
    throw new Error(
      `root-signer binary not found at: ${exePath}. Run 'cargo build' inside 'src-tauri' first.`
    );
  }
  return exePath;
}

function runRootSigner(args: string[]): void {
  const result = spawnSync(resolveRootSignerBinary(), args, { encoding: 'utf8' });
  if (result.error) {
    throw new Error(`root-signer ${args[0]} failed to spawn: ${result.error.message}`);
  }
  if (result.status !== 0) {
    throw new Error(
      `root-signer ${args[0]} exited with code ${result.status}: ${(result.stderr ?? '').trim()}`
    );
  }
}

/**
 * Creates the Root ceremony directory and generates a throwaway Authority Root
 * keypair inside it.
 *
 * MUST be called before the application launches: `GRPC_ROOT_PUBLIC_KEY` is read
 * from the process environment, and a running Windows process cannot be given new
 * environment variables.
 */
export function createRootCeremony(): RootCeremony {
  // Deliberately NOT under the application sandbox: the Root private key must
  // never be readable from application state.
  const dir = mkdtempSync(path.join(os.tmpdir(), 'grpc_e2e_root_'));
  const privateKeyFile = path.join(dir, ROOT_SECRET_FILE);
  const publicKeyFile = path.join(dir, ROOT_PUBLIC_KEY_FILE);

  try {
    // `init` is atomic and refuses to overwrite an existing secret; the fresh
    // mkdtemp guarantees there is none.
    runRootSigner(['init', '--secret-file', privateKeyFile, '--public-key-file', publicKeyFile]);

    if (!existsSync(privateKeyFile)) {
      throw new Error(`root-signer init did not write ${privateKeyFile}`);
    }
    if (!existsSync(publicKeyFile)) {
      throw new Error(`root-signer init did not write ${publicKeyFile}`);
    }

    const publicKey = readFileSync(publicKeyFile, 'utf8').trim();
    const decoded = Buffer.from(publicKey, 'base64');
    // Re-encoding proves canonical padded Base64, not merely a lenient decode.
    if (decoded.length !== ED25519_PUBLIC_KEY_LEN || decoded.toString('base64') !== publicKey) {
      throw new Error(
        `root-signer init produced a public key that is not canonical Base64 for ${ED25519_PUBLIC_KEY_LEN} bytes`
      );
    }

    return {
      dir,
      publicKey,
      privateKeyFile,
      cleanup(): void {
        rmSync(dir, { recursive: true, force: true });
      },
    };
  } catch (err) {
    rmSync(dir, { recursive: true, force: true });
    throw err;
  }
}

// ── Ceremony ──────────────────────────────────────────────────────────────────

/** Inputs for the ceremony. The app is already running and CDP-attached. */
export interface FirstAdminCeremonyContext {
  page: Page;
  driver: TauriDriver;
  rootCeremony: RootCeremony;
}

/** Observable end state of a completed ceremony. */
export interface FirstAdminCeremonyResult {
  /** ADR-0062 identity directory that must own every identity artifact. */
  identityDataDir: string;
  /** Application sandbox owned by `ProcessManager`. */
  sandboxDir: string;
  /** Root-signed, ACTIVE WILAYA certificate installed by `finalize_wilaya_provision`. */
  wilayaCertificate: IdentityCertificateDto;
  /** ADMIN certificate issued by `issue_first_admin_key`. */
  adminCertificate: IdentityCertificateDto;
}

/**
 * Invokes a Tauri command over the real IPC bridge, waiting for the bridge to
 * exist first (the same pattern as `RuntimeContracts`).
 */
async function invoke<T>(
  page: Page,
  command: string,
  args?: Record<string, unknown>
): Promise<T> {
  return (await page.evaluate(
    async ({ cmd, cmdArgs }) => {
      for (let i = 0; i < 50; i++) {
        if ((window as any).__TAURI__ && (window as any).__TAURI__.core) break;
        await new Promise(r => setTimeout(r, 100));
      }
      const core = (window as any).__TAURI__?.core;
      if (!core) {
        throw new Error('Tauri IPC bridge is unavailable');
      }
      return await core.invoke(cmd, cmdArgs);
    },
    { cmd: command, cmdArgs: args }
  )) as T;
}

/**
 * True when `child` is `parent` itself or a descendant of `parent`.
 *
 * Uses `path.relative()` rather than a `startsWith()` prefix test. A prefix test
 * cannot distinguish a real descendant from a sibling that merely shares a name
 * prefix (`...\grpc_e2e_root` vs `...\grpc_e2e_root_x`), which is exactly the
 * shape of the two run-scoped directories this suite creates. `relative()`
 * returns `''` for the same path, an absolute path for a different Windows
 * drive, and a `..`-prefixed path when `child` escapes `parent`.
 */
function isPathContainedBy(child: string, parent: string): boolean {
  const relative = path.relative(path.resolve(parent), path.resolve(child));
  if (relative === '') return true;
  if (path.isAbsolute(relative)) return false;
  return relative !== '..' && !relative.startsWith(`..${path.sep}`);
}

/**
 * Runs the complete first-ADMIN ceremony against a freshly launched app and
 * leaves it authenticated, configured as a WILAYA node, and on the dashboard.
 *
 * Must be called once per test: each `tauriAdminApp` fixture supplies its own
 * sandbox, its own identity directory, and its own Root keypair, so no admin
 * identity is ever shared between tests.
 */
export async function performFirstAdminCeremony(
  ctx: FirstAdminCeremonyContext
): Promise<FirstAdminCeremonyResult> {
  const { page, driver, rootCeremony } = ctx;

  const identityDataDir = driver.getIdentityDataDir();
  const sandboxDir = driver.getTempDir();
  if (!sandboxDir) {
    throw new Error('E2E sandbox directory is unavailable; was the driver started?');
  }

  // Defense in depth: the Root private key must never live in app state, so the
  // ceremony directory must neither be the sandbox nor sit beneath it.
  expect(isPathContainedBy(rootCeremony.dir, sandboxDir)).toBe(false);

  const csrPath = path.join(sandboxDir, CSR_FILE);
  const signedCertPath = path.join(sandboxDir, SIGNED_CERT_FILE);

  // ── Settle ────────────────────────────────────────────────────────────────
  // The SPA performs an initial client-side route redirect once the webview
  // attaches. Evaluating across that navigation destroys the execution context
  // ("Execution context was destroyed"), so wait for the login view to render
  // first — the same guard `ensureLoginIdentity` already applies.
  await page.waitForSelector(AUTH_TAB_SELECTOR, {
    state: 'visible',
    timeout: CEREMONY_STEP_TIMEOUT,
  });

  // ── E. begin_wilaya_provision (pre-auth) ───────────────────────────────────
  // Generates the node keypair, persists `node_identity.key` into the ADR-0062
  // identity directory, and writes the unsigned CSR to `csrPath`.
  const csr = await invoke<IdentityCertificateDto>(page, 'begin_wilaya_provision', {
    requestFilePath: csrPath,
  });
  expect(csr.subject_type).toBe('WILAYA');
  expect(csr.issuer_identity_id).toBeNull();
  // `generate_identity_request` builds the CSR with `status: Active` and no
  // signature: the certificate is only *trustworthy* once the Root signs it.
  expect(csr.status).toBe('ACTIVE');
  // `IdentityCertificate::signature` carries
  // `#[serde(default, skip_serializing_if = "Option::is_none")]`, so an unsigned
  // CSR omits the field entirely (it arrives as `undefined`, not `null`).
  // Coercing proves the security property — no signature is present — without
  // depending on which of the two representations Serde emits.
  expect(csr.signature ?? null).toBeNull();
  expect(existsSync(csrPath)).toBe(true);
  expect(existsSync(path.join(identityDataDir, NODE_KEY_FILE))).toBe(true);
  expect(await invoke<IdentityBootstrapState>(page, 'get_identity_status')).toBe(
    'WAITING_FOR_ROOT_CERTIFICATE'
  );

  // ── F. Offline Root signing (external process) ───────────────────────────
  // `--key-file` (not `--key-hex`) keeps the private key out of the command line.
  runRootSigner([
    'sign',
    '--csr',
    csrPath,
    '--out',
    signedCertPath,
    '--key-file',
    rootCeremony.privateKeyFile,
  ]);
  expect(existsSync(signedCertPath)).toBe(true);

  // ── G. finalize_wilaya_provision (pre-auth) ───────────────────────────────
  // Verifies the Root signature against `GRPC_ROOT_PUBLIC_KEY`, checks the cert
  // public key matches the node key, and installs the ACTIVE WILAYA identity.
  const finalized = await invoke<FinalizeWilayaProvisionResultDto>(
    page,
    'finalize_wilaya_provision',
    { certFilePath: signedCertPath }
  );
  // A fresh node must take the `Provisioned` branch; `AlreadyProvisioned` would
  // mean the ceremony was run twice against one identity directory.
  if (!('Provisioned' in finalized)) {
    throw new Error(
      `finalize_wilaya_provision did not provision the node: ${JSON.stringify(finalized)}`
    );
  }
  const wilayaCertificate = finalized.Provisioned;
  expect(wilayaCertificate.subject_type).toBe('WILAYA');
  expect(wilayaCertificate.status).toBe('ACTIVE');
  expect(await invoke<IdentityBootstrapState>(page, 'get_identity_status')).toBe('WILAYA_ACTIVE');

  // ── H. issue_first_admin_key (pre-auth on a node with no ACTIVE ADMIN) ────
  // Creates the `users` row with an EMPTY password hash and writes `.adminkey`.
  // `set_fleet_admin_password` is deliberately NOT used: that is the separate
  // fleet-admin credential, not the bootstrap identity.
  const adminCertificate = await invoke<IdentityCertificateDto>(
    page,
    'issue_first_admin_key',
    { subjectUsername: E2E_ADMIN_USERNAME, passphrase: E2E_ADMIN_PASSPHRASE }
  );
  expect(adminCertificate.subject_type).toBe('ADMIN');
  expect(existsSync(path.join(identityDataDir, ADMINKEY_FILE))).toBe(true);
  // Empty password hash + present `.adminkey` is exactly the `Ready` projection.
  expect(await invoke<IdentityBootstrapState>(page, 'get_identity_status')).toBe('READY');

  // ── I. Mandatory reload ───────────────────────────────────────────────────
  // The ceremony above ran over IPC, so the frontend's `identityState` is stale
  // and `adminkeyAvailable` is still false. Only a remount re-runs
  // `refreshIdentityStatus()` and re-enables the admin-key tab.
  await page.reload({ waitUntil: 'domcontentloaded' });

  // ── J/K. Real admin-key (Challenge–Response) login through the UI ─────────
  const adminKeyTab = page.locator(AUTH_TAB_SELECTOR).nth(ADMIN_KEY_TAB_INDEX);
  await expect(adminKeyTab).toBeEnabled({ timeout: CEREMONY_STEP_TIMEOUT });
  await adminKeyTab.click();

  await expect(page.locator(PASSPHRASE_INPUT)).toBeVisible({
    timeout: CEREMONY_STEP_TIMEOUT,
  });
  await page.locator(PASSPHRASE_INPUT).fill(E2E_ADMIN_PASSPHRASE);
  await page.locator(ADMIN_KEY_SUBMIT).click();

  // ── L. afterLogin routed an unconfigured node to /configure ───────────────
  // Rendered-heading waits (not raw URL polling) are the mechanism this suite
  // already uses, and they tolerate `svelte-spa-router` hash navigation.
  await page.waitForSelector(CONFIGURE_HEADING, { timeout: CEREMONY_STEP_TIMEOUT });
  expect(page.url()).toContain('configure');

  // ── M. Configure the node as WILAYA (AdminOnly, session from step K) ─────
  await page.locator(WILAYA_CODE_INPUT).fill(E2E_WILAYA_CODE);
  await page.locator(WILAYA_NAME_INPUT).fill(E2E_WILAYA_NAME);
  await page.locator(CONFIGURE_SUBMIT).click();

  // The configure page navigates on a 1500 ms timer; waiting for the rendered
  // dashboard heading respects that instead of racing it.
  await page.waitForSelector(DASHBOARD_HEADING, { timeout: CEREMONY_STEP_TIMEOUT });
  expect(page.url()).toContain('wilaya');

  // ── N. Real secure session, established by the app's own login path ───────
  await RuntimeContracts.assertSecureSessionBootstrapped(page);

  // ── ADR-0062 path isolation ──────────────────────────────────────────────
  // Node key and admin key must live in the per-run identity directory.
  expect(existsSync(path.join(identityDataDir, NODE_KEY_FILE))).toBe(true);
  expect(existsSync(path.join(identityDataDir, ADMINKEY_FILE))).toBe(true);
  // `appkey.age` is optional: the E2E environment supplies `GRPC_APP_KEY`, so the
  // debug lifecycle may legitimately never persist the on-disk store.

  // Nothing identity-bearing may reach the operator's real data directory.
  // Operational data (`GRPC/logs`, backups) is explicitly out of ADR-0062 scope
  // and is therefore NOT asserted here.
  const platformDataDir = process.env.APPDATA
    ? path.join(process.env.APPDATA, GRPC_PLATFORM_DIR_NAME)
    : null;
  if (platformDataDir) {
    for (const artifact of IDENTITY_ARTIFACTS) {
      expect(
        existsSync(path.join(platformDataDir, artifact)),
        `${artifact} leaked into the platform data directory ${platformDataDir}`
      ).toBe(false);
    }
  }

  return { identityDataDir, sandboxDir, wilayaCertificate, adminCertificate };
}
