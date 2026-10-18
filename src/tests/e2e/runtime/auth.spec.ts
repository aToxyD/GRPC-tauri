import { test, expect } from '../fixtures/tauriApp';
import { RuntimeContracts } from '../contracts/runtimeContracts';
import { invokeTauri, performFirstAdminCeremony } from '../helpers/firstAdminCeremony';

test.describe('Tauri Authentication Runtime Lifecycle & Lockout', () => {
  test.skip(process.platform !== 'win32', 'Tauri IPC backend is required and only supported on Windows in E2E tests');

  test('rate limiter blocks access after repeated invalid login attempts', async ({ tauriApp }) => {
    const { page } = tauriApp;

    // The login controls are gated behind `loginEligible` and are intentionally
    // hidden on this fresh, unconfigured node, so the authoritative control is
    // exercised directly over IPC — the exact `login` command the form invokes.
    //
    // Backend contract (`src-tauri/src/commands/auth.rs`,
    // `src-tauri/src/domain/rate_limiter.rs`):
    //   - attempts 1..`DEFAULT_MAX_ATTEMPTS` (5) RESOLVE with `success: false`
    //     and the generic invalid-credentials message: `admin` has no credential
    //     on a fresh node (B6-A seeds no password admin), so each attempt takes
    //     the generic-failure path that records a per-user rate-limiter hit;
    //   - the attempt AT the threshold is REJECTED (the IPC promise throws),
    //     because the per-user gate returns `Err(...)` — never a resolved
    //     lockout `LoginResponse`.
    // Modelling both shapes is required; asserting a resolved lockout can never
    // pass. The fixture starts a fresh sandbox (its own persisted rate-limiter
    // store), so no lockout state leaks in from another test.
    const GENERIC_FAILURE = 'اسم المستخدم أو كلمة المرور غير صحيحة';
    const MAX_ATTEMPTS = 5;

    for (let i = 0; i < MAX_ATTEMPTS; i++) {
      const response = await invokeTauri<{ success: boolean; message: string }>(page, 'login', {
        request: { username: 'admin', password: `wrong_password_${i}` },
      });
      expect(response.success).toBe(false);
      expect(response.message).toContain(GENERIC_FAILURE);
    }

    // The attempt at the threshold rejects; capture the message regardless of the
    // rejection shape (Tauri rejects with the raw backend error value).
    let lockoutMessage = '';
    try {
      await invokeTauri(page, 'login', {
        request: { username: 'admin', password: 'wrong_password_lockout' },
      });
      throw new Error('expected the 6th invalid attempt to be rejected by the rate limiter');
    } catch (error) {
      lockoutMessage = error instanceof Error ? error.message : String(error);
    }
    expect(lockoutMessage).toMatch(/تجاوز الحد/);
  });

  test('successful authentication, session bootstrap, and node redirection', async ({ tauriAdminApp }) => {
    const { page, driver, rootCeremony } = tauriAdminApp;

    // Real first-ADMIN bootstrap ceremony: Root-sign the WILAYA CSR, issue the
    // admin identity key, authenticate through the admin-key UI, and configure
    // the node. Production seeds no password-enabled admin, so a password login
    // cannot reach an authenticated state on a fresh database.
    const ceremony = await performFirstAdminCeremony({ page, driver, rootCeremony });

    // The ceremony leaves the node authenticated, configured, and on the
    // dashboard; assert the identities it actually provisioned.
    expect(ceremony.wilayaCertificate.subject_type).toBe('WILAYA');
    expect(ceremony.wilayaCertificate.status).toBe('ACTIVE');
    expect(ceremony.adminCertificate.subject_type).toBe('ADMIN');

    // Assert session exists in memory/local storage
    await RuntimeContracts.assertSecureSessionBootstrapped(page);
  });
});
