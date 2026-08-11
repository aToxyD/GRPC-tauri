import { describe, it, expect, beforeEach, vi } from 'vitest';

// Mock the Tauri IPC bridge. Every contract function funnels through
// `safeInvoke` -> `invoke` (tauri.ts), so capturing `invoke` calls here proves
// the exact argument keys handed to Tauri's serialization layer for each
// command — the layer where the snake_case/camelCase mismatch lived.
const mockInvoke = vi.fn();
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
}));

import {
  beginWilayaProvision,
  finalizeWilayaProvision,
  issueFirstAdminKey,
  beginUnitProvision,
  signUnitIdentityRequest,
  finalizeUnitProvision,
  installWilayaCertificate,
  beginWilayaRotation,
  finalizeWilayaRotation,
  beginUnitRotation,
  signUnitRotationRequest,
  finalizeUnitRotation,
  beginChallenge,
  completeChallenge,
} from '../../../lib/contracts/identity.contract';
import {
  getLicensingStatus,
  importTrustAnchor,
  importLicense,
  dryRunVerifyLicense,
  verifyLicense,
} from '../../../lib/contracts/licensing.contract';
import { calculateMealRate } from '../../../lib/contracts/consumption.contract';
import { updateFiscalPackageRetentionStatus } from '../../../lib/contracts/fiscal.contract';
import { initializeAppKey } from '../../../lib/contracts/security.contract';

const SNAKE_CASE_KEY = /^[a-z]+_[a-z]/;

/** Find the args object passed to `invoke` for a given command. */
function argsFor(command: string): Record<string, unknown> {
  const call = mockInvoke.mock.calls.find(([name]) => name === command);
  expect(call, `expected invoke('${command}', ...) to have been called`).toBeDefined();
  return (call![1] ?? {}) as Record<string, unknown>;
}

function expectExactKeys(command: string, expected: string[]): void {
  const args = argsFor(command);
  const keys = Object.keys(args).sort();
  expect(keys, `arg keys for ${command}`).toEqual([...expected].sort());
}

describe('IPC contract argument naming (Tauri camelCase serialization)', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('asserts no contract passes a snake_case argument key', async () => {
    // Exercise every command that carries arguments.
    await beginWilayaProvision('/tmp/wilaya-request.json');
    await finalizeWilayaProvision('/tmp/wilaya-signed.json');
    await issueFirstAdminKey('admin', 'passphrase');
    await beginUnitProvision('/tmp/unit-request.json');
    await signUnitIdentityRequest('{"cert":"x"}');
    await finalizeUnitProvision('/tmp/unit-signed.json');
    await installWilayaCertificate('/tmp/wilaya-cert.json');
    await beginWilayaRotation('/tmp/rot-request.json', 'ROTATE');
    await finalizeWilayaRotation('/tmp/rot-signed.json', '/tmp/rot-package.json');
    await beginUnitRotation('/tmp/unit-rot-request.json', 'RE-ISSUE');
    await signUnitRotationRequest('{"cert":"y"}');
    await finalizeUnitRotation('/tmp/unit-rot-signed.json');
    await completeChallenge('session-1', 'passphrase');
    await importTrustAnchor('{"format":"provisioning-v1"}');
    await importLicense('{"artifact":1}');
    await dryRunVerifyLicense('{"artifact":2}');
    await calculateMealRate(100, 2, 3, 4, 5, 6);
    await updateFiscalPackageRetentionStatus('t1', 'ARCHIVED', 'confirm');
    await initializeAppKey('passphrase', '/tmp/backup.json');

    const calls = mockInvoke.mock.calls.map(([name]) => name as string);
    for (const command of calls) {
      const keys = Object.keys(argsFor(command));
      const offenders = keys.filter((k) => SNAKE_CASE_KEY.test(k));
      expect(offenders, `snake_case keys for ${command}`).toEqual([]);
    }
  });

  it('begin_wilaya_provision -> requestFilePath (CSR export path)', async () => {
    await beginWilayaProvision('/tmp/wilaya-request.json');
    expectExactKeys('begin_wilaya_provision', ['requestFilePath']);
  });

  it('finalize_wilaya_provision -> certFilePath', async () => {
    await finalizeWilayaProvision('/tmp/wilaya-signed.json');
    expectExactKeys('finalize_wilaya_provision', ['certFilePath']);
  });

  it('issue_first_admin_key -> subjectUsername + passphrase', async () => {
    await issueFirstAdminKey('admin', 'secret');
    expectExactKeys('issue_first_admin_key', ['subjectUsername', 'passphrase']);
  });

  it('complete_challenge -> sessionId + passphrase', async () => {
    await completeChallenge('session-1', 'secret');
    expectExactKeys('complete_challenge', ['sessionId', 'passphrase']);
  });

  it('import_trust_anchor -> packageJson', async () => {
    await importTrustAnchor('{"format":"provisioning-v1"}');
    expectExactKeys('import_trust_anchor', ['packageJson']);
  });

  it('import_license -> artifactJson', async () => {
    await importLicense('{"artifact":1}');
    expectExactKeys('import_license', ['artifactJson']);
  });

  it('dry_run_verify_license -> artifactJson', async () => {
    await dryRunVerifyLicense('{"artifact":2}');
    expectExactKeys('dry_run_verify_license', ['artifactJson']);
  });

  it('calculate_meal_rate -> camelCase count keys', async () => {
    await calculateMealRate(100, 2, 3, 4, 5, 6);
    expectExactKeys('calculate_meal_rate', [
      'totalCost',
      'staff24hCount',
      'staff8hCount',
      'reservationCount',
      'missionCount',
      'guestCount',
    ]);
  });

  it('update_fiscal_package_retention_status -> transitionId', async () => {
    await updateFiscalPackageRetentionStatus('t1', 'ARCHIVED', 'confirm');
    expectExactKeys('update_fiscal_package_retention_status', [
      'transitionId',
      'status',
      'confirmation',
    ]);
  });

  it('initialize_app_key -> passphrase + exportBackup', async () => {
    await initializeAppKey('secret', '/tmp/backup.json');
    expectExactKeys('initialize_app_key', ['passphrase', 'exportBackup']);
  });

  it('identity file-path commands all use camelCase keys', async () => {
    await beginUnitProvision('/tmp/u.json');
    await signUnitIdentityRequest('{}');
    await finalizeUnitProvision('/tmp/u.json');
    await installWilayaCertificate('/tmp/w.json');
    await beginWilayaRotation('/tmp/r.json', 'ROTATE');
    await finalizeWilayaRotation('/tmp/r.json', '/tmp/p.json');
    await beginUnitRotation('/tmp/ur.json', 'RE-ISSUE');
    await signUnitRotationRequest('{}');
    await finalizeUnitRotation('/tmp/ur.json');
    expectExactKeys('begin_unit_provision', ['requestFilePath']);
    expectExactKeys('sign_unit_identity_request', ['requestJson']);
    expectExactKeys('finalize_unit_provision', ['certFilePath']);
    expectExactKeys('install_wilaya_certificate', ['certFilePath']);
    expectExactKeys('begin_wilaya_rotation', ['requestFilePath', 'operation']);
    expectExactKeys('finalize_wilaya_rotation', [
      'certFilePath',
      'rotationPackagePath',
    ]);
    expectExactKeys('begin_unit_rotation', ['requestFilePath', 'operation']);
    expectExactKeys('sign_unit_rotation_request', ['requestJson']);
    expectExactKeys('finalize_unit_rotation', ['certFilePath']);
  });

  it('arg-less commands are invoked without an args object', async () => {
    await getLicensingStatus();
    await verifyLicense();
    await beginChallenge();
    for (const [name, args] of mockInvoke.mock.calls) {
      expect(args, `${name} should carry no arguments`).toBeUndefined();
    }
  });
});
