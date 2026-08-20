/**
 * B8 lifecycle UI (ADR-0040/0045) — LoginPage (UNIT side): identity_access
 * package import + login credential distinction (SEC-013 Phase 3).
 *
 * Verifies the frontend contract invocation only. The backend first-import
 * predicates and SEC-010 signature verification remain authoritative; the
 * import never auto-logs-in and never exposes credential contents.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, fireEvent, screen, waitFor } from '@testing-library/svelte';
import LoginPage from '../../pages/LoginPage.svelte';
import { currentUser } from '../../lib/session';
import { get } from 'svelte/store';

const mockLogin = vi.fn();
const mockIsConfigured = vi.fn();
const mockGetSettings = vi.fn();
const mockGetIdentityStatus = vi.fn();
const mockGetSecurityStatus = vi.fn();
const mockImportUnitNodePackage = vi.fn();
const mockImportIdentityAccessPackage = vi.fn();
const mockInstallWilayaCertificate = vi.fn();
const mockBeginChallenge = vi.fn();
const mockCompleteChallenge = vi.fn();
const mockOpenFile = vi.fn();

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
  importIdentityAccessPackage: (...args: any[]) => mockImportIdentityAccessPackage(...args),
  getIdentityStatus: (...args: any[]) => mockGetIdentityStatus(...args),
  getSecurityStatus: (...args: any[]) => mockGetSecurityStatus(...args),
  beginWilayaProvision: vi.fn(),
  finalizeWilayaProvision: vi.fn(),
  issueFirstAdminKey: vi.fn(),
  beginUnitProvision: vi.fn(),
  finalizeUnitProvision: vi.fn(),
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

describe('LoginPage — B8 import + credential distinction (SEC-013 Phase 3)', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    currentUser.set(null);
    mockIsConfigured.mockResolvedValue(true);
    mockGetSecurityStatus.mockResolvedValue({ requires_action: false });
  });

  it('shows the B8 import section on a fresh configured UNIT after the .unit ceremony', async () => {
    mockGetSettings.mockResolvedValue({ configured: true, node_type: 'UNIT', unit_code: 'U1' });
    mockGetIdentityStatus.mockResolvedValue('UNIT_ACTIVE');

    render(LoginPage);

    await waitFor(() => {
      expect(screen.getByText('استيراد حساب المسؤول العام (B8)')).toBeInTheDocument();
    });
    // The section explains the distinction from the `.unit` package.
    expect(screen.getByText(/تختلف حزمة الحسابات عن حزمة/)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /استيراد حزمة الحسابات \(B8\)/ })).toBeInTheDocument();
  });

  it('import invokes importIdentityAccessPackage with the chosen file and never auto-logs-in', async () => {
    mockGetSettings.mockResolvedValue({ configured: true, node_type: 'UNIT', unit_code: 'U1' });
    mockGetIdentityStatus.mockResolvedValue('UNIT_ACTIVE');
    mockOpenFile.mockResolvedValue('/tmp/grpc-identity-access-U1.sync');
    mockImportIdentityAccessPackage.mockResolvedValue({
      admin_updated: true,
      user_updated: true,
      user_renamed: false,
      package_id: 'ia-pkg-1',
      imported_by: 'admin',
      timestamp: '2026-08-20T00:00:00Z',
    });

    render(LoginPage);

    const importButton = await screen.findByRole('button', { name: /استيراد حزمة الحسابات \(B8\)/ });
    await fireEvent.click(importButton);

    await waitFor(() => {
      expect(mockOpenFile).toHaveBeenCalledWith(
        expect.objectContaining({ filters: [{ name: 'حزمة الحسابات (B8)', extensions: ['sync'] }] })
      );
      expect(mockImportIdentityAccessPackage).toHaveBeenCalledWith('/tmp/grpc-identity-access-U1.sync');
    });
    expect(
      screen.getByText(/تم استيراد حزمة الحسابات \(B8\) — حساب المسؤول العام متاح محلياً/)
    ).toBeInTheDocument();
    // No auto-login: no session, no redirect.
    expect(get(currentUser)).toBeNull();
    expect(mockPush).not.toHaveBeenCalled();
  });

  it('displays the import failure without hiding the section', async () => {
    mockGetSettings.mockResolvedValue({ configured: true, node_type: 'UNIT', unit_code: 'U1' });
    mockGetIdentityStatus.mockResolvedValue('UNIT_ACTIVE');
    mockOpenFile.mockResolvedValue('/tmp/grpc-identity-access-U1.sync');
    mockImportIdentityAccessPackage.mockRejectedValue(
      new Error('حزمة الحسابات غير صالحة (التوقيع غير متطابق)')
    );

    render(LoginPage);

    const importButton = await screen.findByRole('button', { name: /استيراد حزمة الحسابات \(B8\)/ });
    await fireEvent.click(importButton);

    await waitFor(() => {
      expect(screen.getByText(/خطأ في استيراد حزمة الحسابات: حزمة الحسابات غير صالحة/)).toBeInTheDocument();
    });
    expect(screen.getByRole('button', { name: /استيراد حزمة الحسابات \(B8\)/ })).toBeInTheDocument();
  });

  it('password tab explains the fleet admin password; admin-key tab explains the passphrase', async () => {
    mockGetSettings.mockResolvedValue({ configured: true, node_type: 'WILAYA', wilaya_code: '16' });
    mockGetIdentityStatus.mockResolvedValue('ADMIN_PROVISIONED');

    render(LoginPage);

    // Password tab hint (ADR-0050 normal path uses the B8 fleet credential).
    await waitFor(() => {
      expect(screen.getByText('كلمة مرور المسؤول العام')).toBeInTheDocument();
      expect(screen.getByText(/وليست كلمة مرور المفتاح الإداري/)).toBeInTheDocument();
    });

    // Admin-key tab hint (challenge passphrase ≠ account password).
    await fireEvent.click(screen.getByRole('tab', { name: 'المفتاح الإداري' }));
    await waitFor(() => {
      expect(screen.getByText('مختلفة')).toBeInTheDocument();
      expect(screen.getByText(/عن كلمة مرور الحساب/)).toBeInTheDocument();
    });
  });
});