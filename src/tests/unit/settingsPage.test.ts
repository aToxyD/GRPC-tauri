/**
 * SEC-014 Phase 4 + SEC-021 — SettingsPage: the production home for the
 * post-provisioning Admin credential and Admin-Only account synchronization
 * (ADR-0045 / ADR-0051, Decision D1).
 *
 * Verifies frontend behavior only — the backend remains the sole authority
 * for hashing, signing, encryption, and authorization. No secret is ever
 * logged, persisted, or rendered after submission; no import ever creates a
 * session or triggers automatic navigation. Per SEC-033 the admin_access
 * surface on the WILAYA side is fleet-level: there is NO UNIT selector —
 * the backend enumerates the authoritative target set itself — and the
 * UNIT side guarantees that the `.unit`-provisioned operator account stays
 * untouched.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, fireEvent, screen, waitFor } from '@testing-library/svelte';
import SettingsPage from '../../pages/SettingsPage.svelte';

let currentUserValue: { username: string; role: string } | null = null;

const mockGetSettings = vi.fn();
const mockSetFleetAdminPassword = vi.fn();
const mockExportAdminAccessPackage = vi.fn();
const mockImportAdminAccessPackage = vi.fn();
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
  setFleetAdminPassword: (...args: any[]) => mockSetFleetAdminPassword(...args),
  exportAdminAccessPackage: (...args: any[]) => mockExportAdminAccessPackage(...args),
  importAdminAccessPackage: (...args: any[]) => mockImportAdminAccessPackage(...args),
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

describe('SettingsPage — WILAYA Admin (SEC-014 Phase 4 / SEC-021)', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    currentUserValue = wilayaAdmin;
    mockGetSettings.mockResolvedValue({
      configured: true,
      node_type: 'WILAYA',
      wilaya_code: '31',
    });
  });

  it('renders the account security section with NO UNIT target selector (SEC-033)', async () => {
    render(SettingsPage);

    await waitFor(() => {
      expect(screen.getByText('أمان الحسابات')).toBeInTheDocument();
      expect(screen.getByText('مزامنة حساب المدير العام (admin)')).toBeInTheDocument();
    });
    expect(screen.getByLabelText(/^كلمة مرور المسؤول العام/)).toBeInTheDocument();
    expect(screen.getByLabelText(/تأكيد كلمة مرور المسؤول العام/)).toBeInTheDocument();
    // Credential-distinction notice (Admin Key passphrase ≠ fleet password).
    expect(screen.getByText('مختلفة تماماً')).toBeInTheDocument();
    // SEC-033: fleet-level export — the renderer must not select (or
    // default-select) a UNIT security target.
    expect(screen.queryByRole('combobox', { name: /الوحدة الهدف/ })).toBeNull();
    // The WILAYA side never renders the UNIT import action.
    expect(screen.queryByRole('button', { name: /استيراد حزمة حساب المدير العام/ })).toBeNull();
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

  it('exports the admin_access package fleet-level with no target parameter (SEC-033)', async () => {
    mockExportAdminAccessPackage.mockResolvedValue({
      file_path: '/tmp/admin.sync',
      record_count: 1,
      success: true,
      message: 'ok',
      file_hash: 'abc123',
    });
    mockSaveFile.mockResolvedValue('/tmp/grpc-admin-access.sync');
    render(SettingsPage);

    await screen.findByText('مزامنة حساب المدير العام (admin)');

    await fireEvent.click(screen.getByRole('button', { name: /تصدير حزمة حساب المدير العام الموقّعة والمشفّرة/ }));

    await waitFor(() => {
      expect(mockSaveFile).toHaveBeenCalledWith(
        expect.objectContaining({ defaultPath: 'grpc-admin-access.sync' })
      );
      expect(mockExportAdminAccessPackage).toHaveBeenCalledTimes(1);
      // SEC-033: exactly ONE argument — the renderer cannot choose a target.
      expect(mockExportAdminAccessPackage).toHaveBeenCalledWith('/tmp/grpc-admin-access.sync');
    });
    // The UI only reports counts — never credential contents.
    expect(screen.getByText(/تم تصدير حزمة حساب المدير العام/)).toBeInTheDocument();
    expect(screen.queryByText(/argon2|password_hash|admin_password_hash/i)).toBeNull();
  });

  it('displays the fail-closed backend rejection for admin_access export', async () => {
    mockExportAdminAccessPackage.mockRejectedValue(
      new Error('كلمة مرور المسؤول العام لم تُضبط بعد؛ حدّثها أولاً')
    );
    mockSaveFile.mockResolvedValue('/tmp/grpc-admin-access.sync');
    render(SettingsPage);

    await screen.findByText('مزامنة حساب المدير العام (admin)');

    await fireEvent.click(screen.getByRole('button', { name: /تصدير حزمة حساب المدير العام الموقّعة والمشفّرة/ }));

    await waitFor(() => {
      expect(
        screen.getByText(/كلمة مرور المسؤول العام لم تُضبط بعد/)
      ).toBeInTheDocument();
    });
    expect(mockExportAdminAccessPackage).toHaveBeenCalledWith('/tmp/grpc-admin-access.sync');
  });
});

describe('SettingsPage — UNIT (SEC-014 Phase 4 / SEC-021)', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    currentUserValue = unitUser;
    mockGetSettings.mockResolvedValue({
      configured: true,
      node_type: 'UNIT',
      unit_code: 'U1',
    });
  });

  it('renders the admin sync import section and never the fleet password form', async () => {
    render(SettingsPage);

    await waitFor(() => {
      expect(screen.getByText('مزامنة حساب المدير العام (admin)')).toBeInTheDocument();
    });
    expect(screen.getByRole('button', { name: /استيراد حزمة حساب المدير العام/ })).toBeInTheDocument();
    expect(screen.queryByLabelText(/^كلمة مرور المسؤول العام/)).toBeNull();
    expect(screen.queryByRole('button', { name: /تصدير حزمة حساب المدير العام/ })).toBeNull();
  });

  it('imports the admin_access package with explicit guidance and no session creation or redirect', async () => {
    mockOpenFile.mockResolvedValue('/tmp/grpc-admin-access.sync');
    mockImportAdminAccessPackage.mockResolvedValue({
      admin_updated: true,
      package_id: 'aa-pkg-1',
      imported_by: 'unit-operator',
      timestamp: '2026-08-22T00:00:00Z',
    });
    render(SettingsPage);

    const importButton = await screen.findByRole('button', { name: /استيراد حزمة حساب المدير العام/ });
    await fireEvent.click(importButton);

    await waitFor(() => {
      expect(mockOpenFile).toHaveBeenCalledWith(
        expect.objectContaining({ filters: [{ name: 'حزمة حساب المدير العام', extensions: ['sync'] }] })
      );
      expect(mockImportAdminAccessPackage).toHaveBeenCalledWith('/tmp/grpc-admin-access.sync');
    });
    await waitFor(() => {
      expect(screen.getByText(/تمت مزامنة حساب المدير العام بنجاح/)).toBeInTheDocument();
    });
    // Explicit operator guidance: sign out, then sign in as admin with the
    // fleet password issued on the WILAYA node.
    expect(screen.getByText(/باسم admin باستخدام كلمة مرور المسؤول العام/)).toBeInTheDocument();
    // The `.unit` operator account is untouched.
    expect(screen.getByText(/حساب مشغّل الوحدة الخاص بك لم يُمَسّ/)).toBeInTheDocument();
    // No session is created and no automatic navigation occurs.
    expect(currentUserValue).toBe(unitUser);
    expect(mockPush).not.toHaveBeenCalled();
  });

  it('explains that synchronization touches ONLY the admin account (ADR-0051 §5)', async () => {
    const { container } = render(SettingsPage);

    // The guidance is static and always visible in the UNIT sync section.
    await waitFor(() => {
      expect(screen.getByText('مزامنة حساب المدير العام (admin)')).toBeInTheDocument();
    });

    // Operator-preservation guarantee, stated up front.
    expect(container.textContent).toContain('لا تُعدّل حساب مشغّل الوحدة المحلي إطلاقًا');
    // The `.unit` provisioning file remains the credential source of truth.
    expect(container.textContent).toContain('.unit');
    // No SEC-017 "credential transformation" copy may survive — the old
    // semantics renamed/re-passworded the unit account and are now forbidden.
    expect(container.textContent).not.toContain('باسم المستخدم القياسي');
    expect(container.textContent).not.toContain('كلمة مروره هي كلمة مرور مستخدم');
    expect(container.textContent).not.toContain('لم يعودا صالحين لتسجيل الدخول');
  });

  it('displays the import failure without hiding the section', async () => {
    mockOpenFile.mockResolvedValue('/tmp/grpc-admin-access.sync');
    mockImportAdminAccessPackage.mockRejectedValue(
      new Error('حزمة الحسابات غير صالحة (التوقيع غير متطابق)')
    );
    render(SettingsPage);

    const importButton = await screen.findByRole('button', { name: /استيراد حزمة حساب المدير العام/ });
    await fireEvent.click(importButton);

    await waitFor(() => {
      expect(
        screen.getByText(/خطأ في استيراد حزمة حساب المدير العام: حزمة الحسابات غير صالحة/)
      ).toBeInTheDocument();
    });
    expect(screen.getByRole('button', { name: /استيراد حزمة حساب المدير العام/ })).toBeInTheDocument();
    expect(mockPush).not.toHaveBeenCalled();
  });

  it('renders the same import section for a UNIT Admin session', async () => {
    currentUserValue = unitAdmin;
    render(SettingsPage);

    await waitFor(() => {
      expect(screen.getByText('مزامنة حساب المدير العام (admin)')).toBeInTheDocument();
    });
    expect(screen.getByRole('button', { name: /استيراد حزمة حساب المدير العام/ })).toBeInTheDocument();
    expect(screen.queryByLabelText(/^كلمة مرور المسؤول العام/)).toBeNull();
  });
});

describe('SettingsPage — access control (SEC-014 Phase 4 / SEC-021)', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockGetSettings.mockResolvedValue({
      configured: true,
      node_type: 'WILAYA',
      wilaya_code: '31',
    });
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
      expect(screen.queryByRole('button', { name: /استيراد حزمة حساب المدير العام/ })).toBeNull();
    });
  });
});
