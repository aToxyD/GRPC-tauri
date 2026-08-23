/**
 * SEC-014 Phase 4 regression — LoginPage no longer hosts the identity_access
 * (B8) import; it keeps the `.unit` bootstrap ceremony and the credential
 * distinction hints. The B8 import lives on SettingsPage and requires an
 * authenticated session (backend-authoritative).
 */
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/svelte';
import LoginPage from '../../pages/LoginPage.svelte';
import { currentUser } from '../../lib/session';

const mockIsConfigured = vi.fn();
const mockGetSettings = vi.fn();
const mockGetIdentityStatus = vi.fn();
const mockGetSecurityStatus = vi.fn();

vi.mock('../../lib/tauri', () => ({
  safeInvoke: vi.fn(),
  openFile: vi.fn(),
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
  login: vi.fn(),
  isConfigured: (...args: any[]) => mockIsConfigured(...args),
  getSettings: (...args: any[]) => mockGetSettings(...args),
  importUnitNodePackage: vi.fn(),
  importIdentityAccessPackage: vi.fn(),
  getIdentityStatus: (...args: any[]) => mockGetIdentityStatus(...args),
  getSecurityStatus: (...args: any[]) => mockGetSecurityStatus(...args),
  beginWilayaProvision: vi.fn(),
  finalizeWilayaProvision: vi.fn(),
  issueFirstAdminKey: vi.fn(),
  beginUnitProvision: vi.fn(),
  finalizeUnitProvision: vi.fn(),
  installWilayaCertificate: vi.fn(),
  beginChallenge: vi.fn(),
  completeChallenge: vi.fn(),
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

describe('LoginPage — B8 import relocated to Settings (SEC-014 Phase 4)', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    currentUser.set(null);
    mockIsConfigured.mockResolvedValue(true);
    mockGetSecurityStatus.mockResolvedValue({ requires_action: false });
    mockGetSettings.mockResolvedValue({ configured: true, node_type: 'UNIT', unit_code: 'U1' });
    mockGetIdentityStatus.mockResolvedValue('UNIT_ACTIVE');
  });

  it('no longer renders the identity_access (B8) import section', async () => {
    render(LoginPage);

    await waitFor(() => {
      expect(screen.getByText('تسجيل الدخول')).toBeInTheDocument();
    });
    expect(
      screen.queryByRole('button', { name: /استيراد حزمة الحسابات/ })
    ).toBeNull();
    expect(screen.queryByText(/تختلف حزمة الحسابات عن حزمة/)).toBeNull();
  });

  it('keeps the `.unit` configuration ceremony intact', async () => {
    // The pre-configuration `.unit` flow renders only while the node is not
    // yet configured.
    mockIsConfigured.mockResolvedValue(false);
    render(LoginPage);

    await screen.findByText('لم يتم تكوين العقدة بعد');
    expect(
      await screen.findByRole('button', { name: /الخطوة 2: استيراد حزمة التكوين \(\.unit\)/ })
    ).toBeInTheDocument();
  });

  it('password tab points operators to the settings page for the fleet password', async () => {
    render(LoginPage);

    await waitFor(() => {
      expect(screen.getByText('كلمة مرور المسؤول العام')).toBeInTheDocument();
      expect(screen.getByText(/يتم تعيين كلمة مرور المسؤول العام من صفحة الإعدادات/)).toBeInTheDocument();
    });
  });
});

describe('LoginPage — ADR-0052 role-aware identity selection (SEC-026)', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    currentUser.set(null);
    mockIsConfigured.mockResolvedValue(true);
    mockGetSecurityStatus.mockResolvedValue({ requires_action: false });
    mockGetSettings.mockResolvedValue({ configured: true, node_type: 'UNIT', unit_code: 'U1' });
    mockGetIdentityStatus.mockResolvedValue('UNIT_ACTIVE');
  });

  it('UNIT nodes choose between the canonical operator and the local admin', async () => {
    render(LoginPage);

    const select = (await screen.findByLabelText('المستخدم')) as HTMLSelectElement;
    await waitFor(() => expect(select.value).toBe('user'));
    const options = Array.from(select.options).map((o) => o.value);
    expect(options).toEqual(['user', 'admin']);
    // Free-text username entry is gone — the identity set is fixed.
    expect(screen.queryByPlaceholderText('أدخل اسم المستخدم')).toBeNull();
  });

  it('WILAYA pins the identity to the fleet admin, read-only', async () => {
    mockGetSettings.mockResolvedValue({
      configured: true,
      node_type: 'WILAYA',
      wilaya_code: '16',
    });
    render(LoginPage);

    const input = await screen.findByLabelText('المسؤول العام (admin)');
    expect((input as HTMLInputElement).value).toBe('admin');
    expect(input as HTMLInputElement).toHaveAttribute('readonly');
    expect(screen.queryByRole('combobox', { name: 'المستخدم' })).toBeNull();
  });
});
