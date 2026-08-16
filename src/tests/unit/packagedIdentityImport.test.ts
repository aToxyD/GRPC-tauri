/**
 * Packaged-identity `.unit` import path (ADR-0044 / RFC §3.12) — LoginPage.
 *
 * Covers the fresh-UNIT import ceremony for a packaged UNIT identity:
 * - Test 1: a fresh (unconfigured) UNIT sees the packaged-identity
 *   informational text during the `.unit` import ceremony, BEFORE import.
 * - Test 2: `.unit` import stays disabled while the WILAYA trust anchor is
 *   absent (B8 anchor-first; the backend predicate remains authoritative).
 * - Test 3: after the WILAYA anchor is installed the `.unit` import becomes
 *   available.
 * - Test 4: a successful packaged import refreshes the identity projection to
 *   `UNIT_ACTIVE` and renders the login-ready UI.
 *
 * Presentation-only: this test does not mock or invent cryptographic behavior.
 * It verifies the frontend contract/API invocation (importUnitNodePackage) and
 * the resulting UI state. The embedded certificate/private-key contents are
 * never observable on the frontend by design.
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
const mockBeginChallenge = vi.fn();
const mockCompleteChallenge = vi.fn();

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
  getIdentityStatus: (...args: any[]) => mockGetIdentityStatus(...args),
  getSecurityStatus: (...args: any[]) => mockGetSecurityStatus(...args),
  beginWilayaProvision: (...args: any[]) => mockBeginWilayaProvision(...args),
  finalizeWilayaProvision: (...args: any[]) => mockFinalizeWilayaProvision(...args),
  issueFirstAdminKey: (...args: any[]) => mockIssueFirstAdminKey(...args),
  beginUnitProvision: (...args: any[]) => mockBeginUnitProvision(...args),
  finalizeUnitProvision: (...args: any[]) => mockFinalizeUnitProvision(...args),
  installWilayaCertificate: (...args: any[]) => mockInstallWilayaCertificate(...args),
  beginChallenge: (...args: any[]) => mockBeginChallenge(...args),
  completeChallenge: (...args: any[]) => mockCompleteChallenge(...args),
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

describe('Packaged-identity `.unit` import path (ADR-0044 / RFC §3.12)', () => {
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
    mockImportUnitNodePackage.mockResolvedValue({
      unit_id: 'unit-1',
      unit_code: 'U1',
      unit_name: 'Unit 1',
      username: 'unit_user',
      file_hash: 'pkg-hash',
      imported_by: 'system',
      timestamp: '2026-08-16T00:00:00Z',
    });
  });

  it('Test 1: fresh UNIT sees the packaged-identity info during the `.unit` import ceremony', async () => {
    render(LoginPage);

    await waitFor(() => {
      expect(screen.getByText('لم يتم تكوين العقدة بعد')).toBeInTheDocument();
    });

    // The packaged-identity informational text is rendered in the
    // `!isAppConfigured` import ceremony block — visible to a fresh UNIT
    // BEFORE a successful import (F-002).
    expect(
      screen.getByText(/تُعدّ عقدة WILAYA هوية الوحدة ضمن حزمة \.unit/)
    ).toBeInTheDocument();

    // No private-key material is ever presented to the user.
    expect(screen.queryByText(/المفتاح الخاص/)).not.toBeNull();
    expect(screen.queryByText(/unit_private_key/)).toBeNull();
    expect(screen.queryByText(/BEGIN ED25519|PRIVATE KEY/i)).toBeNull();
  });

  it('Test 2: `.unit` import remains disabled while the WILAYA trust anchor is absent', async () => {
    render(LoginPage);

    await waitFor(() => {
      expect(screen.getByRole('button', { name: /الخطوة 1: تثبيت شهادة WILAYA \(مرساة الثقة\)/ })).toBeInTheDocument();
    });

    // B8 anchor-first: no installed WILAYA anchor → `unitAnchorInstalled`
    // is false → the `.unit` import is disabled. The backend predicate
    // (`B8FirstImportPredicatesService::verify_unit_v2_acceptance`) remains
    // the authoritative enforcement.
    const importButton = screen.getByRole('button', { name: /استيراد حزمة التكوين \(\.unit\)/ });
    expect(importButton).toBeDisabled();
    expect(importButton).toHaveAttribute('aria-disabled', 'true');
  });

  it('Test 3: after the WILAYA anchor is installed the `.unit` import becomes available', async () => {
    // onMount reads UNINITIALIZED; the anchor install then flips the
    // projection to WILAYA_ACTIVE (ACTIVE WILAYA cert on a fresh node).
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

    const importButton = screen.getByRole('button', { name: /استيراد حزمة التكوين \(\.unit\)/ });
    await waitFor(() => {
      expect(importButton).toBeEnabled();
    });
  });

  it('Test 4: packaged import refreshes identity state → UNIT_ACTIVE → login-ready UI', async () => {
    mockGetIdentityStatus
      .mockResolvedValueOnce('UNINITIALIZED')
      .mockResolvedValueOnce('WILAYA_ACTIVE')
      .mockResolvedValue('UNIT_ACTIVE');
    mockGetSettings
      .mockResolvedValueOnce({ configured: false, node_type: 'UNCONFIGURED' })
      .mockResolvedValue({ configured: true, node_type: 'UNIT' });
    mockOpenFile
      .mockResolvedValueOnce('/tmp/wilaya-cert.json')
      .mockResolvedValueOnce('/tmp/unit-1_package.unit');

    render(LoginPage);

    // Step 1: install the WILAYA trust anchor.
    const anchorButton = await screen.findByRole('button', { name: /الخطوة 1: تثبيت شهادة WILAYA \(مرساة الثقة\)/ });
    await fireEvent.click(anchorButton);
    await waitFor(() => {
      expect(mockInstallWilayaCertificate).toHaveBeenCalledWith('/tmp/wilaya-cert.json');
    });

    // Step 2: import the packaged `.unit`.
    const importButton = screen.getByRole('button', { name: /استيراد حزمة التكوين \(\.unit\)/ });
    await waitFor(() => {
      expect(importButton).toBeEnabled();
    });
    await fireEvent.click(importButton);

    // The frontend calls the import API and then refreshes the identity
    // projection (the backend installs the packaged identity and is the sole
    // source of the embedded certificate/private-key).
    await waitFor(() => {
      expect(mockImportUnitNodePackage).toHaveBeenCalledWith('/tmp/unit-1_package.unit');
      // refreshIdentityStatus → getIdentityStatus returns UNIT_ACTIVE
      expect(mockGetIdentityStatus).toHaveBeenCalled();
    });

    // Login-ready UI: the identity is represented as UNIT_ACTIVE.
    await waitFor(() => {
      expect(screen.getByText('هوية الوحدة مفعلة')).toBeInTheDocument();
      expect(screen.getByText(/هوية الوحدة مفعلة — يمكنك تسجيل الدخول\./)).toBeInTheDocument();
    });
  });
});