/**
 * C-3 (Gate-2) regression — UNIT bootstrap anchor-first ceremony (B8 / ADR-0045).
 *
 * Covers the LoginPage fresh-node UI ordering:
 * - Test 1: a fresh (unconfigured) UNIT can SEE the WILAYA trust-anchor
 *   installation step BEFORE the `.unit` import.
 * - Test 2: `.unit` import is unavailable until the anchor requirement is
 *   satisfied (backend `verify_unit_v2_acceptance` remains the authoritative
 *   enforcement — covered by `src-tauri/tests/b8_first_import_tests.rs` B1).
 * - Test 3: after a successful anchor install, `.unit` import becomes
 *   available and the normal ceremony continues.
 *
 * Presentation-only: this test does not re-implement the backend predicate; it
 * verifies the UI surfaces the anchor step first and gates the import button on
 * the existing `get_identity_status` projection (WILAYA_ACTIVE = an ACTIVE
 * WILAYA trust anchor is installed on a fresh node).
 */
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, fireEvent, screen, waitFor } from '@testing-library/svelte';
import LoginPage from '../../pages/LoginPage.svelte';

const mockLogin = vi.fn();
const mockIsConfigured = vi.fn();
const mockGetSettings = vi.fn();
const mockImportUnitNodePackage = vi.fn();
const mockGetIdentityStatus = vi.fn();
const mockGetSecurityStatus = vi.fn();
const mockInstallWilayaCertificate = vi.fn();
const mockOpenFile = vi.fn();
const mockBeginWilayaProvision = vi.fn();
const mockFinalizeWilayaProvision = vi.fn();
const mockIssueFirstAdminKey = vi.fn();
const mockBeginUnitProvision = vi.fn();
const mockFinalizeUnitProvision = vi.fn();

vi.mock('../../lib/tauri', () => ({
  safeInvoke: vi.fn(),
  openFile: (...args: any[]) => mockOpenFile(...args),
  saveFile: vi.fn(),
  getAppWindow: () => ({
    maximize: vi.fn(),
    unmaximize: vi.fn(),
    setResizable: vi.fn(),
    setMinSize: vi.fn(),
    setSize: vi.fn(),
    setMaximizable: vi.fn(),
    isMaximized: vi.fn().mockResolvedValue(true),
    center: vi.fn(),
  }),
  createLogicalSize: vi.fn().mockReturnValue({}),
}));

vi.mock('../../lib/contracts', () => ({
  login: (...args: any[]) => mockLogin(...args),
  isConfigured: (...args: any[]) => mockIsConfigured(...args),
  getSettings: (...args: any[]) => mockGetSettings(...args),
  importUnitNodePackage: (...args: any[]) => mockImportUnitNodePackage(...args),
  importIdentityAccessPackage: vi.fn(),
  getIdentityStatus: (...args: any[]) => mockGetIdentityStatus(...args),
  getSecurityStatus: (...args: any[]) => mockGetSecurityStatus(...args),
  beginWilayaProvision: (...args: any[]) => mockBeginWilayaProvision(...args),
  finalizeWilayaProvision: (...args: any[]) => mockFinalizeWilayaProvision(...args),
  issueFirstAdminKey: (...args: any[]) => mockIssueFirstAdminKey(...args),
  beginUnitProvision: (...args: any[]) => mockBeginUnitProvision(...args),
  finalizeUnitProvision: (...args: any[]) => mockFinalizeUnitProvision(...args),
  installWilayaCertificate: (...args: any[]) => mockInstallWilayaCertificate(...args),
}));

vi.mock('@tauri-apps/plugin-dialog', () => ({
  open: vi.fn(),
}));

vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({ maximize: vi.fn() }),
}));

const mockPush = vi.fn();
vi.mock('svelte-spa-router', () => ({
  push: (...args: any[]) => mockPush(...args),
}));

describe('UNIT bootstrap anchor-first ceremony (C-3 / B8 / ADR-0045)', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockIsConfigured.mockResolvedValue(false);
    mockGetSettings.mockResolvedValue({ configured: false, node_type: 'UNCONFIGURED' });
    mockGetSecurityStatus.mockResolvedValue({ requires_action: false });
    mockGetIdentityStatus.mockResolvedValue('UNINITIALIZED');
    mockOpenFile.mockResolvedValue('/tmp/wilaya-cert.json');
    mockInstallWilayaCertificate.mockResolvedValue({
      Installed: { identity_id: 'anchor-1', subject_type: 'WILAYA' },
    });
  });

  it('Test 1: fresh UNIT sees the WILAYA trust-anchor step before `.unit` import', async () => {
    render(LoginPage);

    await waitFor(() => {
      expect(screen.getByText('لم يتم تكوين العقدة بعد')).toBeInTheDocument();
    });

    // The anchor step is presented first and is actionable on the fresh node.
    const anchorButton = screen.getByRole('button', { name: /الخطوة 1: تثبيت شهادة WILAYA \(مرساة الثقة\)/ });
    expect(anchorButton).toBeInTheDocument();
    expect(anchorButton).toBeEnabled();

    // `.unit` import is present but unavailable until the anchor is installed.
    const importButton = screen.getByRole('button', { name: /استيراد حزمة التكوين \(\.unit\)/ });
    expect(importButton).toBeInTheDocument();
    expect(importButton).toBeDisabled();
  });

  it('Test 2: `.unit` import is unavailable before the anchor requirement is satisfied', async () => {
    render(LoginPage);

    await waitFor(() => {
      expect(screen.getByRole('button', { name: /الخطوة 1: تثبيت شهادة WILAYA \(مرساة الثقة\)/ })).toBeInTheDocument();
    });

    // No anchor installed yet → WILAYA_ACTIVE not projected → the `.unit`
    // import step is unavailable (native `disabled` button: browser does not
    // dispatch click events on it). The backend predicate is the authority:
    // `B8FirstImportPredicatesService::verify_unit_v2_acceptance` rejects a
    // `.unit` V2 without an installed anchor (B1, `b8_first_import_tests.rs`).
    const importButton = screen.getByRole('button', { name: /استيراد حزمة التكوين \(\.unit\)/ });
    expect(importButton).toBeDisabled();
    expect(importButton).toHaveAttribute('aria-disabled', 'true');
  });

  it('Test 3: after anchor install, `.unit` import becomes available and the ceremony continues', async () => {
    // onMount reads UNINITIALIZED; the anchor install then flips the projection
    // to WILAYA_ACTIVE (ACTIVE WILAYA cert present on the fresh node).
    mockGetIdentityStatus
      .mockResolvedValueOnce('UNINITIALIZED')
      .mockResolvedValue('WILAYA_ACTIVE');

    render(LoginPage);

    const anchorButton = await screen.findByRole('button', { name: /الخطوة 1: تثبيت شهادة WILAYA \(مرساة الثقة\)/ });
    await fireEvent.click(anchorButton);

    await waitFor(() => {
      expect(mockOpenFile).toHaveBeenCalled();
      expect(mockInstallWilayaCertificate).toHaveBeenCalledWith('/tmp/wilaya-cert.json');
    });

    // Import step becomes available once the anchor is installed.
    const importButton = screen.getByRole('button', { name: /استيراد حزمة التكوين \(\.unit\)/ });
    await waitFor(() => {
      expect(importButton).toBeEnabled();
    });

    // The ceremony continues: `.unit` import proceeds.
    await fireEvent.click(importButton);
    expect(mockImportUnitNodePackage).toHaveBeenCalledWith('/tmp/wilaya-cert.json');
  });
});