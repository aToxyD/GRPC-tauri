/**
 * SEC-014 Phase 4 — SettingsPage: the production home for the post-
 * provisioning Admin credential + B8 account lifecycle (ADR-0040/0045).
 *
 * Verifies frontend behavior only — the backend remains the sole authority
 * for hashing, signing, encryption, and authorization. No secret is ever
 * logged, persisted, or rendered after submission; no import ever creates a
 * session or triggers automatic navigation.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, fireEvent, screen, waitFor } from '@testing-library/svelte';
import SettingsPage from '../../pages/SettingsPage.svelte';

let currentUserValue: { username: string; role: string } | null = null;

const mockGetSettings = vi.fn();
const mockListUnits = vi.fn();
const mockSetFleetAdminPassword = vi.fn();
const mockExportIdentityAccessPackage = vi.fn();
const mockImportIdentityAccessPackage = vi.fn();
const mockOpenFile = vi.fn();
const mockSaveFile = vi.fn();
const mockCurrentUserSubscribe = vi.fn((listener: (value: unknown) => void) => {
  listener(currentUserValue);
  return () => {};
});
const mockThemeSubscribe = vi.fn((listener: (value: unknown) => void) => {
  listener('dark');
  return () => {};
});

vi.mock('../../lib/session', () => ({
  currentUser: { subscribe: (l: any) => mockCurrentUserSubscribe(l) },
  logout: vi.fn(),
}));

vi.mock('../../lib/theme', () => ({
  theme: { subscribe: (l: any) => mockThemeSubscribe(l) },
  toggleTheme: vi.fn(),
}));

vi.mock('lucide-svelte', () => {
  const Stub = function () {};
  return { Sun: Stub, Moon: Stub };
});

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
  importIdentityAccessPackage: (...args: any[]) => mockImportIdentityAccessPackage(...args),
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

const wilayaAdmin = { username: 'admin', role: 'Admin' };
const unitUser = { username: 'unit-operator', role: 'User' };
const unitAdmin = { username: 'unit-admin', role: 'Admin' };
const wilayaUser = { username: 'wilaya-operator', role: 'User' };

const units = [
  { id: 'unit-1', code: 'U1', name: 'Unit 1', wilaya_code: '31', user_id: 'user-1', created_at: '2026-08-01T00:00:00Z' },
  { id: 'unit-2', code: 'U2', name: 'Unit 2', wilaya_code: '31', user_id: 'user-2', created_at: '2026-08-01T00:00:00Z' },
];

describe('SettingsPage — WILAYA Admin (SEC-014 Phase 4)', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    currentUserValue = wilayaAdmin;
    mockGetSettings.mockResolvedValue({
      configured: true,
      node_type: 'WILAYA',
      wilaya_code: '31',
    });
    mockListUnits.mockResolvedValue(units);
  });

  it('renders the account security and B8 sections with a populated unit selector', async () => {
    render(SettingsPage);

    await waitFor(() => {
      expect(screen.getByText('أمان الحسابات')).toBeInTheDocument();
      expect(screen.getByText('مزامنة الحسابات (B8)')).toBeInTheDocument();
    });
    expect(screen.getByLabelText(/^كلمة مرور المسؤول العام/)).toBeInTheDocument();
    expect(screen.getByLabelText(/تأكيد كلمة مرور المسؤول العام/)).toBeInTheDocument();
    // Credential-distinction notice (Admin Key passphrase ≠ fleet password).
    expect(screen.getByText('مختلفة تماماً')).toBeInTheDocument();
    // Unit selector populated for export targeting.
    expect(await screen.findByText('U1 - Unit 1')).toBeInTheDocument();
    expect(screen.getByText('U2 - Unit 2')).toBeInTheDocument();
    // The WILAYA side never renders the UNIT import action.
    expect(screen.queryByRole('button', { name: /استيراد حزمة الحسابات/ })).toBeNull();
  });

  it('sets the fleet password via IPC and clears both fields afterwards', async () => {
    mockSetFleetAdminPassword.mockResolvedValue(undefined);
    render(SettingsPage);

    const passwordInput = await screen.findByLabelText(/^كلمة مرور المسؤول العام/);
    const confirmInput = screen.getByLabelText(/تأكيد كلمة مرور المسؤول العام/);
    await fireEvent.input(passwordInput, { target: { value: 'FleetPass123' } });
    await fireEvent.input(confirmInput, { target: { value: 'FleetPass123' } });

    await fireEvent.click(screen.getByRole('button', { name: /تعيين كلمة مرور المسؤول العام/ }));

    await waitFor(() => {
      expect(mockSetFleetAdminPassword).toHaveBeenCalledTimes(1);
      expect(mockSetFleetAdminPassword).toHaveBeenCalledWith('FleetPass123');
    });
    await waitFor(() => {
      expect(screen.getByLabelText(/^كلمة مرور المسؤول العام/)).toHaveValue('');
      expect(screen.getByLabelText(/تأكيد كلمة مرور المسؤول العام/)).toHaveValue('');
    });
    expect(screen.queryByDisplayValue('FleetPass123')).toBeNull();
    expect(screen.getByText(/تم تعيين كلمة مرور المسؤول العام/)).toBeInTheDocument();
  });

  it('rejects a too-short password without invoking the command', async () => {
    render(SettingsPage);

    const passwordInput = await screen.findByLabelText(/^كلمة مرور المسؤول العام/);
    const confirmInput = screen.getByLabelText(/تأكيد كلمة مرور المسؤول العام/);
    await fireEvent.input(passwordInput, { target: { value: 'short' } });
    await fireEvent.input(confirmInput, { target: { value: 'short' } });

    await fireEvent.click(screen.getByRole('button', { name: /تعيين كلمة مرور المسؤول العام/ }));

    await waitFor(() => {
      expect(screen.getByText('كلمة المرور قصيرة جداً — يجب أن تكون 8 أحرف على الأقل')).toBeInTheDocument();
    });
    expect(mockSetFleetAdminPassword).not.toHaveBeenCalled();
  });

  it('rejects a mismatched confirmation without invoking the command', async () => {
    render(SettingsPage);

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
    render(SettingsPage);

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

  it('exports the B8 package with the selected unit code and chosen destination', async () => {
    mockExportIdentityAccessPackage.mockResolvedValue({
      file_path: '/tmp/ia.sync',
      record_count: 2,
      success: true,
      message: 'ok',
      file_hash: 'abc123',
    });
    mockSaveFile.mockResolvedValue('/tmp/grpc-identity-access-U1.sync');
    render(SettingsPage);

    await screen.findByText('مزامنة الحسابات (B8)');

    await fireEvent.click(screen.getByRole('button', { name: /تصدير حزمة الحسابات \(B8\) الموقّعة والمشفّرة/ }));

    await waitFor(() => {
      expect(mockSaveFile).toHaveBeenCalledWith(
        expect.objectContaining({ defaultPath: 'grpc-identity-access-U1.sync' })
      );
      expect(mockExportIdentityAccessPackage).toHaveBeenCalledTimes(1);
      expect(mockExportIdentityAccessPackage).toHaveBeenCalledWith('U1', '/tmp/grpc-identity-access-U1.sync');
    });
    // The UI only reports counts — never credential contents.
    expect(screen.getByText(/تم تصدير حزمة الحسابات \(B8\) للوحدة U1/)).toBeInTheDocument();
    expect(screen.queryByText(/argon2|password_hash|admin_password_hash/i)).toBeNull();
  });

  it('displays the fail-closed backend rejection for B8 export', async () => {
    mockExportIdentityAccessPackage.mockRejectedValue(
      new Error('كلمة مرور المسؤول العام لم تُضبط بعد؛ حدّثها أولاً')
    );
    mockSaveFile.mockResolvedValue('/tmp/grpc-identity-access-U1.sync');
    render(SettingsPage);

    await screen.findByText('مزامنة الحسابات (B8)');

    await fireEvent.click(screen.getByRole('button', { name: /تصدير حزمة الحسابات \(B8\) الموقّعة والمشفّرة/ }));

    await waitFor(() => {
      expect(
        screen.getByText(/كلمة مرور المسؤول العام لم تُضبط بعد/)
      ).toBeInTheDocument();
    });
    expect(mockExportIdentityAccessPackage).toHaveBeenCalled();
  });
});

describe('SettingsPage — UNIT (SEC-014 Phase 4)', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    currentUserValue = unitUser;
    mockGetSettings.mockResolvedValue({
      configured: true,
      node_type: 'UNIT',
      unit_code: 'U1',
    });
  });

  it('renders the B8 import section and never the fleet password form', async () => {
    render(SettingsPage);

    await waitFor(() => {
      expect(screen.getByText('مزامنة الحسابات (B8)')).toBeInTheDocument();
    });
    expect(screen.getByRole('button', { name: /استيراد حزمة الحسابات \(B8\)/ })).toBeInTheDocument();
    expect(screen.queryByLabelText(/^كلمة مرور المسؤول العام/)).toBeNull();
    expect(screen.queryByRole('button', { name: /تصدير حزمة الحسابات/ })).toBeNull();
  });

  it('imports the B8 package with explicit guidance and no session creation or redirect', async () => {
    mockOpenFile.mockResolvedValue('/tmp/grpc-identity-access-U1.sync');
    mockImportIdentityAccessPackage.mockResolvedValue({
      admin_updated: true,
      user_updated: true,
      user_renamed: false,
      package_id: 'ia-pkg-1',
      imported_by: 'unit-operator',
      timestamp: '2026-08-20T00:00:00Z',
    });
    render(SettingsPage);

    const importButton = await screen.findByRole('button', { name: /استيراد حزمة الحسابات \(B8\)/ });
    await fireEvent.click(importButton);

    await waitFor(() => {
      expect(mockOpenFile).toHaveBeenCalledWith(
        expect.objectContaining({ filters: [{ name: 'حزمة الحسابات (B8)', extensions: ['sync'] }] })
      );
      expect(mockImportIdentityAccessPackage).toHaveBeenCalledWith('/tmp/grpc-identity-access-U1.sync');
    });
    await waitFor(() => {
      expect(screen.getByText(/تم استيراد حزمة الحسابات بنجاح/)).toBeInTheDocument();
    });
    // Explicit operator guidance: sign out, then sign in as admin with the
    // fleet password issued on the WILAYA node.
    expect(screen.getByText(/باسم admin باستخدام كلمة مرور المسؤول العام/)).toBeInTheDocument();
    // No session is created and no automatic navigation occurs.
    expect(currentUserValue).toBe(unitUser);
    expect(mockPush).not.toHaveBeenCalled();
  });

  it('explains the post-B8 credential transformation (SEC-017 Issue A)', async () => {
    const { container } = render(SettingsPage);

    // The guidance is static and always visible in the UNIT B8 section.
    await waitFor(() => {
      expect(screen.getByText('مزامنة الحسابات (B8)')).toBeInTheDocument();
    });

    // Canonical username after import.
    expect(container.textContent).toContain('باسم المستخدم القياسي');
    // Password becomes the WILAYA-issued unit-user password; obtain it there.
    expect(container.textContent).toContain('كلمة مروره هي كلمة مرور مستخدم');
    expect(container.textContent).toContain('الوحدة الصادرة عن عقدة WILAYA');
    expect(container.textContent).toContain('الحصول عليها من عقدة WILAYA');
    // Previous credentials are no longer valid on this node.
    expect(container.textContent).toContain('لم يعودا صالحين لتسجيل الدخول');
  });

  it('warns that the unit account password may become the WILAYA-issued one', async () => {
    mockOpenFile.mockResolvedValue('/tmp/grpc-identity-access-U1.sync');
    mockImportIdentityAccessPackage.mockResolvedValue({
      admin_updated: true,
      user_updated: true,
      user_renamed: false,
      package_id: 'ia-pkg-2',
      imported_by: 'unit-operator',
      timestamp: '2026-08-20T00:00:00Z',
    });
    render(SettingsPage);

    await fireEvent.click(await screen.findByRole('button', { name: /استيراد حزمة الحسابات \(B8\)/ }));

    await waitFor(() => {
      expect(screen.getByText(/كلمة مروره هي كلمة مرور مستخدم الوحدة الصادرة عن/)).toBeInTheDocument();
    });
  });

  it('displays the import failure without hiding the section', async () => {
    mockOpenFile.mockResolvedValue('/tmp/grpc-identity-access-U1.sync');
    mockImportIdentityAccessPackage.mockRejectedValue(
      new Error('حزمة الحسابات غير صالحة (التوقيع غير متطابق)')
    );
    render(SettingsPage);

    const importButton = await screen.findByRole('button', { name: /استيراد حزمة الحسابات \(B8\)/ });
    await fireEvent.click(importButton);

    await waitFor(() => {
      expect(screen.getByText(/خطأ في استيراد حزمة الحسابات: حزمة الحسابات غير صالحة/)).toBeInTheDocument();
    });
    expect(screen.getByRole('button', { name: /استيراد حزمة الحسابات \(B8\)/ })).toBeInTheDocument();
    expect(mockPush).not.toHaveBeenCalled();
  });

  it('renders the same import section for a UNIT Admin session', async () => {
    currentUserValue = unitAdmin;
    render(SettingsPage);

    await waitFor(() => {
      expect(screen.getByText('مزامنة الحسابات (B8)')).toBeInTheDocument();
    });
    expect(screen.getByRole('button', { name: /استيراد حزمة الحسابات \(B8\)/ })).toBeInTheDocument();
    expect(screen.queryByLabelText(/^كلمة مرور المسؤول العام/)).toBeNull();
  });
});

describe('SettingsPage — access control (SEC-014 Phase 4)', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockGetSettings.mockResolvedValue({
      configured: true,
      node_type: 'WILAYA',
      wilaya_code: '31',
    });
    mockListUnits.mockResolvedValue(units);
  });

  it('redirects an unauthenticated visitor to the entry route', async () => {
    currentUserValue = null;
    render(SettingsPage);

    await waitFor(() => {
      expect(mockPush).toHaveBeenCalledWith('/login');
    });
  });

  it('redirects a WILAYA User away from the settings surface', async () => {
    currentUserValue = wilayaUser;
    render(SettingsPage);

    await waitFor(() => {
      expect(mockPush).toHaveBeenCalledWith('/wilaya');
    });
    await waitFor(() => {
      expect(screen.queryByLabelText(/^كلمة مرور المسؤول العام/)).toBeNull();
      expect(screen.queryByRole('button', { name: /استيراد حزمة الحسابات/ })).toBeNull();
    });
  });
});
