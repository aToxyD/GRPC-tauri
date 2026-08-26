/**
 * SEC-014 Phase 4 regression — SyncPage is operational sync only.
 *
 * The fleet admin password form and the identity_access (B8) export moved to
 * SettingsPage; SyncPage keeps the daily/monthly/stock package imports. The
 * backend authorization model is unchanged (ManageAccountSync /
 * ExportIdentityAccessPackage → Wilaya + AdminOnly).
 */
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/svelte';
import SyncPage from '../../pages/SyncPage.svelte';

const mockGetSettings = vi.fn();
const mockListUnits = vi.fn();
const mockCurrentUserSubscribe = vi.fn((listener: (value: unknown) => void) => {
  listener({ username: 'admin', role: 'Admin' });
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
  listenToResize: vi.fn().mockResolvedValue(() => {}),
}));

vi.mock('../../lib/contracts', () => ({
  getSettings: (...args: any[]) => mockGetSettings(...args),
  listUnits: (...args: any[]) => mockListUnits(...args),
  setFleetAdminPassword: vi.fn(),
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

describe('SyncPage — B8 lifecycle relocated to Settings (SEC-014 Phase 4)', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockGetSettings.mockResolvedValue({
      configured: true,
      node_type: 'WILAYA',
      wilaya_code: '31',
    });
    mockListUnits.mockResolvedValue([
      { id: 'unit-1', code: 'U1', name: 'Unit 1', wilaya_code: '31', user_id: 'user-1', created_at: '2026-08-01T00:00:00Z' },
    ]);
  });

  it('no longer renders the fleet admin password form', async () => {
    render(SyncPage);

    await screen.findAllByRole('button', { name: /مسار المزامنة الرسمي/ });
    expect(screen.queryByLabelText(/^كلمة مرور المسؤول العام/)).toBeNull();
    expect(screen.queryByLabelText(/تأكيد كلمة مرور المسؤول العام/)).toBeNull();
    expect(
      screen.queryByRole('button', { name: /تعيين كلمة مرور المسؤول العام/ })
    ).toBeNull();
  });

  it('no longer renders the identity_access (B8) export action', async () => {
    render(SyncPage);

    await screen.findAllByRole('button', { name: /مسار المزامنة الرسمي/ });
    expect(screen.queryByText('حسابات العقد (B8)')).toBeNull();
    expect(
      screen.queryByRole('button', { name: /تصدير حزمة الحسابات/ })
    ).toBeNull();
  });

  it('keeps the operational package-import surface intact', async () => {
    render(SyncPage);

    expect(
      (await screen.findAllByRole('button', { name: /مسار المزامنة الرسمي/ })).length
    ).toBe(2);
    expect(screen.getAllByRole('button', { name: /مسار المزامنة الرسمي/ }).length).toBe(2);
    expect(
      screen.getByRole('button', { name: /استيراد الحزمة المشفرة والموقعة/ })
    ).toBeInTheDocument();
  });
});
