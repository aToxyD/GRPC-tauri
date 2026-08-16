/**
 * اختبارات صفحة إعداد الأمان (AppSecurityPage) — APPKEY-001 UI Remediation
 *
 * تغطي:
 * - حالة `source === 'none'` (عقدة جديدة): إرشاد مسار الأسطول + تحذير فشل `.unit`
 * - حالة `source === 'env'`: إظهار حالة مفتاح الأسطول دون كشف السر
 * - حالة `source === 'store'`: بقاء سلوك فتح المخزن المحلي
 * - بقاء التوليد المحلي متاحاً كخيار صريح للعقد المستقلة / WILAYA
 * - عدم عرض أي قيمة سرية (AGE-SECRET-KEY) في DOM
 */
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/svelte';
import AppSecurityPage from '../../../pages/AppSecurityPage.svelte';

const mockGetSecurityStatus = vi.fn();
const mockInitializeAppKey = vi.fn();
const mockUnlockAppKey = vi.fn();
const mockPush = vi.fn();

vi.mock('../../../lib/tauri', () => ({
  safeInvoke: vi.fn(),
  saveFile: vi.fn(),
  getAppWindow: () => ({
    setResizable: vi.fn(),
    setMaximizable: vi.fn(),
    isMaximized: vi.fn().mockResolvedValue(true),
    unmaximize: vi.fn(),
    setSize: vi.fn(),
    center: vi.fn(),
  }),
  createLogicalSize: vi.fn().mockReturnValue({}),
}));

vi.mock('../../../lib/contracts', () => ({
  getSecurityStatus: (...args: any[]) => mockGetSecurityStatus(...args),
  initializeAppKey: (...args: any[]) => mockInitializeAppKey(...args),
  unlockAppKey: (...args: any[]) => mockUnlockAppKey(...args),
}));

vi.mock('svelte-spa-router', () => ({
  push: (...args: any[]) => mockPush(...args),
}));

function status(overrides: Partial<{ provisioned: boolean; unlocked: boolean; store_path: string; source: string; requires_action: boolean }> = {}) {
  return {
    provisioned: false,
    unlocked: false,
    store_path: '/tmp/grpc/appkey.age',
    source: 'none',
    requires_action: true,
    ...overrides,
  };
}

describe('AppSecurityPage — APPKEY-001 fleet provisioning UX', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('source=none: يعرض إرشاد مسار الأسطول (GRPC_APP_KEY / WILAYA) وتحذير فشل .unit', async () => {
    mockGetSecurityStatus.mockResolvedValue(status({ source: 'none' }));

    const { container } = render(AppSecurityPage);

    await waitFor(() => {
      expect(container.textContent).toContain('GRPC_APP_KEY');
    });
    expect(container.textContent).toContain('نفس قيمة مفتاح WILAYA');
    expect(container.textContent).toContain('فشل فك تشفير حزمة .unit');
    expect(container.textContent).toContain('قبل أول تشغيل');
    expect(container.textContent).toContain('أعد تشغيل التطبيق');
    expect(container.textContent).toContain('أداة التوفير المحمولة');
  });

  it('source=none: يبقي التوليد المحلي متاحاً كخيار صريح (عقدة مستقلة / WILAYA)', async () => {
    mockGetSecurityStatus.mockResolvedValue(status({ source: 'none' }));

    render(AppSecurityPage);

    await waitFor(() => {
      expect(screen.getByRole('button', { name: /إنشاء المفتاح وفتح التطبيق/ })).toBeInTheDocument();
    });
    expect(screen.getByText(/عقدة مستقلة \/ WILAYA/)).toBeInTheDocument();
  });

  it('source=env: يعرض حالة مفتاح الأسطول دون كشف السر ولا يعرض التوليد المحلي', async () => {
    mockGetSecurityStatus.mockResolvedValue(
      status({ source: 'env', provisioned: true, unlocked: true, requires_action: false }),
    );

    const { container } = render(AppSecurityPage);

    await waitFor(() => {
      expect(container.textContent).toContain('مفتاح الأسطول متوفر عبر GRPC_APP_KEY');
    });
    expect(screen.queryByRole('button', { name: /إنشاء المفتاح وفتح التطبيق/ })).not.toBeInTheDocument();
    expect(container.textContent).not.toContain('AGE-SECRET-KEY');
  });

  it('source=store: يحافظ على سلوك فتح المخزن المحلي', async () => {
    mockGetSecurityStatus.mockResolvedValue(
      status({ source: 'store', provisioned: true, unlocked: false, requires_action: true }),
    );

    const { container } = render(AppSecurityPage);

    await waitFor(() => {
      expect(screen.getByRole('button', { name: /فتح التطبيق/ })).toBeInTheDocument();
    });
    expect(container.textContent).toContain('مخزن المفتاح المحلي متوفر');
    expect(container.textContent).toContain('appkey.age');
    expect(screen.queryByRole('button', { name: /إنشاء المفتاح وفتح التطبيق/ })).not.toBeInTheDocument();
  });

  it('لا يعرض أي قيمة سرية في DOM بأي حالة', async () => {
    mockGetSecurityStatus.mockResolvedValue(status({ source: 'none' }));

    const { container } = render(AppSecurityPage);

    await waitFor(() => {
      expect(container.textContent).toContain('GRPC_APP_KEY');
    });
    expect(container.textContent).not.toContain('AGE-SECRET-KEY-1');
    expect(container.textContent).not.toContain('AGE-SECRET-KEY');
    expect(container.textContent).not.toMatch(/x25519\s*1/);
  });
});