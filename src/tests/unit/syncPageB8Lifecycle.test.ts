/**
 * B8 lifecycle UI (ADR-0040) — SyncPage (WILAYA side): fleet admin password
 * initialization + identity_access package export (SEC-013 Phase 3).
 *
 * Verifies the frontend contract invocation only — the backend remains the
 * sole authority for hashing, signing, encryption, and authorization
 * (ManageAccountSync / ExportIdentityAccessPackage → Wilaya + AdminOnly).
 * No password is ever logged, persisted, or rendered after submission, and
 * no package contents are exposed in the UI.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, fireEvent, screen, waitFor } from '@testing-library/svelte';
import SyncPage from '../../pages/SyncPage.svelte';

const mockGetSettings = vi.fn();
const mockListUnits = vi.fn();
const mockSetFleetAdminPassword = vi.fn();
const mockExportIdentityAccessPackage = vi.fn();
const mockOpenFile = vi.fn();
const mockSaveFile = vi.fn();
const mockLogout = vi.fn();
const mockCurrentUserSubscribe = vi.fn();
const mockThemeSubscribe = vi.fn();

vi.mock('../../lib/session', () => ({
  currentUser: { subscribe: (...args: any[]) => mockCurrentUserSubscribe(...args) },
  logout: (...args: any[]) => mockLogout(...args),
}));

vi.mock('../../lib/theme', () => ({
  theme: { subscribe: (...args: any[]) => mockThemeSubscribe(...args) },
  toggleTheme: vi.fn(),
}));

vi.mock('lucide-svelte', () => ({
  Sun: { render: () => null },
  Moon: { render: () => null },
}));

vi.mock('../../lib/tauri', () => ({
  safeInvoke: vi.fn(),
  openFile: (...args: any[]) => mockOpenFile(...args),
  saveFile: (...args: any[]) => mockSaveFile(...args),
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
  listenToResize: vi.fn().mockResolvedValue(() => {}),
}));

vi.mock('../../lib/contracts', () => ({
  getSettings: (...args: any[]) => mockGetSettings(...args),
  listUnits: (...args: any[]) => mockListUnits(...args),
  setFleetAdminPassword: (...args: any[]) => mockSetFleetAdminPassword(...args),
  exportIdentityAccessPackage: (...args: any[]) => mockExportIdentityAccessPackage(...args),
  importDailyReportPackage: vi.fn(),
  importMonthlySummaryPackage: vi.fn(),
  importStockMovementsPackage: vi.fn(),
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
  link: vi.fn(),
}));

describe('SyncPage — B8 lifecycle (SEC-013 Phase 3)', () => {
  const units = [
    { id: 'unit-1', code: 'U1', name: 'Unit 1', wilaya_code: '31', user_id: 'user-1', created_at: '2026-08-01T00:00:00Z' },
    { id: 'unit-2', code: 'U2', name: 'Unit 2', wilaya_code: '31', user_id: 'user-2', created_at: '2026-08-01T00:00:00Z' },
  ];

  beforeEach(() => {
    vi.clearAllMocks();
    mockCurrentUserSubscribe.mockImplementation((listener: (value: unknown) => void) => {
      listener(null);
      return () => {};
    });
    mockThemeSubscribe.mockImplementation((listener: (value: unknown) => void) => {
      listener('dark');
      return () => {};
    });
    mockGetSettings.mockResolvedValue({
      configured: true,
      node_type: 'WILAYA',
      wilaya_code: '31',
    });
    mockListUnits.mockResolvedValue(units);
  });

  it('renders the fleet admin password fields and the credential distinction', async () => {
    render(SyncPage);

    await waitFor(() => {
      expect(screen.getByText('حسابات العقد (B8)')).toBeInTheDocument();
    });
    expect(screen.getByLabelText(/^كلمة مرور المسؤول العام/)).toBeInTheDocument();
    expect(screen.getByLabelText(/تأكيد كلمة مرور المسؤول العام/)).toBeInTheDocument();
    // Credential-distinction notice (Admin Key passphrase ≠ fleet password).
    expect(screen.getByText('مختلفة تماماً')).toBeInTheDocument();
    expect(screen.getByText('كلمة مرور المفتاح الإداري')).toBeInTheDocument();
    // Password fields never prefill or display a secret.
    expect(screen.getByLabelText(/^كلمة مرور المسؤول العام/)).toHaveValue('');
  });

  it('calls setFleetAdminPassword with the entered password and clears the fields', async () => {
    mockSetFleetAdminPassword.mockResolvedValue(undefined);
    render(SyncPage);

    const passwordInput = await screen.findByLabelText(/^كلمة مرور المسؤول العام/);
    const confirmInput = screen.getByLabelText(/تأكيد كلمة مرور المسؤول العام/);
    await fireEvent.input(passwordInput, { target: { value: 'FleetPass123' } });
    await fireEvent.input(confirmInput, { target: { value: 'FleetPass123' } });

    await fireEvent.click(screen.getByRole('button', { name: /تعيين كلمة مرور المسؤول العام/ }));

    await waitFor(() => {
      expect(mockSetFleetAdminPassword).toHaveBeenCalledWith('FleetPass123');
    });
    await waitFor(() => {
      expect(screen.getByLabelText(/^كلمة مرور المسؤول العام/)).toHaveValue('');
      expect(screen.getByLabelText(/تأكيد كلمة مرور المسؤول العام/)).toHaveValue('');
    });
    expect(screen.queryByDisplayValue('FleetPass123')).toBeNull();
    expect(
      screen.getByText(/تم تعيين كلمة مرور المسؤول العام/)
    ).toBeInTheDocument();
  });

  it('rejects a mismatched confirmation without invoking the command', async () => {
    render(SyncPage);

    const passwordInput = await screen.findByLabelText(/^كلمة مرور المسؤول العام/);
    const confirmInput = screen.getByLabelText(/تأكيد كلمة مرور المسؤول العام/);
    await fireEvent.input(passwordInput, { target: { value: 'FleetPass123' } });
    await fireEvent.input(confirmInput, { target: { value: 'FleetPass124' } });

    await fireEvent.click(screen.getByRole('button', { name: /تعيين كلمة مرور المسؤول العام/ }));

    await waitFor(() => {
      expect(screen.getByText('كلمتا المرور غير متطابقتين')).toBeInTheDocument();
    });
    expect(mockSetFleetAdminPassword).not.toHaveBeenCalled();
  });

  it('displays the backend rejection for setFleetAdminPassword', async () => {
    mockSetFleetAdminPassword.mockRejectedValue(new Error('مطلوب تسجيل الدخول كمسؤول (WILAYA)'));
    render(SyncPage);

    const passwordInput = await screen.findByLabelText(/^كلمة مرور المسؤول العام/);
    const confirmInput = screen.getByLabelText(/تأكيد كلمة مرور المسؤول العام/);
    await fireEvent.input(passwordInput, { target: { value: 'FleetPass123' } });
    await fireEvent.input(confirmInput, { target: { value: 'FleetPass123' } });

    await fireEvent.click(screen.getByRole('button', { name: /تعيين كلمة مرور المسؤول العام/ }));

    await waitFor(() => {
      expect(screen.getByText(/مطلوب تسجيل الدخول كمسؤول \(WILAYA\)/)).toBeInTheDocument();
    });
    expect(mockSetFleetAdminPassword).toHaveBeenCalledWith('FleetPass123');
  });

  it('export invokes exportIdentityAccessPackage with the selected unit code and file', async () => {
    mockExportIdentityAccessPackage.mockResolvedValue({
      file_path: '/tmp/ia.sync',
      record_count: 2,
      success: true,
      message: 'ok',
      file_hash: 'abc123',
    });
    mockSaveFile.mockResolvedValue('/tmp/grpc-identity-access-U1.sync');
    render(SyncPage);

    await screen.findByText('حسابات العقد (B8)');

    await fireEvent.click(screen.getByRole('button', { name: /تصدير حزمة الحسابات \(B8\) الموقّعة والمشفّرة/ }));

    await waitFor(() => {
      expect(mockSaveFile).toHaveBeenCalledWith(
        expect.objectContaining({ defaultPath: 'grpc-identity-access-U1.sync' })
      );
    });
    await waitFor(() => {
      expect(mockExportIdentityAccessPackage).toHaveBeenCalledWith('U1', '/tmp/grpc-identity-access-U1.sync');
    });
    // The UI only reports counts — never credential contents.
    expect(screen.getByText(/تم تصدير حزمة الحسابات \(B8\) للوحدة U1/)).toBeInTheDocument();
    expect(screen.queryByText(/FleetPass123|argon2|password_hash|admin_password_hash/i)).toBeNull();
  });

  it('displays the backend rejection for B8 export (fail-closed preserved)', async () => {
    mockExportIdentityAccessPackage.mockRejectedValue(
      new Error('كلمة مرور المسؤول العام لم تُضبط بعد؛ حدّثها أولاً')
    );
    mockSaveFile.mockResolvedValue('/tmp/grpc-identity-access-U1.sync');
    render(SyncPage);

    await screen.findByText('حسابات العقد (B8)');

    await fireEvent.click(screen.getByRole('button', { name: /تصدير حزمة الحسابات \(B8\) الموقّعة والمشفّرة/ }));

    await waitFor(() => {
      expect(
        screen.getByText(/كلمة مرور المسؤول العام لم تُضبط بعد/)
      ).toBeInTheDocument();
    });
    expect(mockExportIdentityAccessPackage).toHaveBeenCalled();
  });
});