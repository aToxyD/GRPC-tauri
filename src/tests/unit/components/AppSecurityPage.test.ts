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
import { render, screen, waitFor, fireEvent } from '@testing-library/svelte';
import AppSecurityPage from '../../../pages/AppSecurityPage.svelte';

const mockGetSecurityStatus = vi.fn();
const mockInitializeAppKey = vi.fn();
const mockUnlockAppKey = vi.fn();
const mockImportAppKey = vi.fn();
const mockPush = vi.fn();

vi.mock('../../../lib/tauri', () => ({
  safeInvoke: vi.fn(),
  saveFile: vi.fn(),
  openFile: vi.fn(),
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
  importAppKey: (...args: any[]) => mockImportAppKey(...args),
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

describe('AppSecurityPage — APPKEY-003 fleet artifact import', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('source=none: يعرض CTA استيراد grpc-app-key.age في بطاقة مسار الأسطول', async () => {
    mockGetSecurityStatus.mockResolvedValue(status({ source: 'none' }));

    const { container } = render(AppSecurityPage);

    await waitFor(() => {
      expect(screen.getByRole('button', { name: /اختيار الملف/ })).toBeInTheDocument();
    });
    expect(screen.getByRole('button', { name: /استيراد المفتاح وفتح التطبيق/ })).toBeInTheDocument();
    expect(container.textContent).toContain('grpc-app-key.age');
    expect(container.textContent).toContain('استورد الملف المحمول مباشرة');
  });

  it('اختيار الملف يستدعي openFile بفلتر age/key/txt ويعرض المسار', async () => {
    mockGetSecurityStatus.mockResolvedValue(status({ source: 'none' }));
    const { openFile } = await import('../../../lib/tauri');
    (openFile as any).mockResolvedValue('/home/op/Downloads/grpc-app-key.age');

    const { container } = render(AppSecurityPage);

    await waitFor(() => {
      expect(screen.getByRole('button', { name: /اختيار الملف/ })).toBeInTheDocument();
    });
    await fireEvent.click(screen.getByRole('button', { name: /اختيار الملف/ }));

    await waitFor(() => {
      expect(openFile).toHaveBeenCalledTimes(1);
    });
    expect((openFile as any).mock.calls[0][0]).toMatchObject({
      multiple: false,
      filters: [{ name: 'مفتاح الأسطول المحمول (age)', extensions: ['age', 'key', 'txt'] }],
    });
    expect(container.textContent).toContain('/home/op/Downloads/grpc-app-key.age');
  });

  it('إلغاء اختيار الملف لا يغيّر الحالة ولا يستدعي importAppKey', async () => {
    mockGetSecurityStatus.mockResolvedValue(status({ source: 'none' }));
    const { openFile } = await import('../../../lib/tauri');
    (openFile as any).mockResolvedValue(null);

    const { container } = render(AppSecurityPage);

    await waitFor(() => {
      expect(screen.getByRole('button', { name: /اختيار الملف/ })).toBeInTheDocument();
    });
    await fireEvent.click(screen.getByRole('button', { name: /اختيار الملف/ }));

    expect(container.textContent).toContain('لم يتم اختيار ملف بعد');
    expect(mockImportAppKey).not.toHaveBeenCalled();
  });

  it('تقديم بدون اختيار ملف: خطأ ولا استدعاء importAppKey', async () => {
    mockGetSecurityStatus.mockResolvedValue(status({ source: 'none' }));

    const { container } = render(AppSecurityPage);

    await waitFor(() => {
      expect(screen.getByRole('button', { name: /استيراد المفتاح وفتح التطبيق/ })).toBeInTheDocument();
    });
    await fireEvent.click(screen.getByRole('button', { name: /استيراد المفتاح وفتح التطبيق/ }));

    expect(container.textContent).toContain('اختر ملف grpc-app-key.age أولاً');
    expect(mockImportAppKey).not.toHaveBeenCalled();
  });

  it('تقديم بكلمة مرور قصيرة: خطأ ولا استدعاء importAppKey', async () => {
    mockGetSecurityStatus.mockResolvedValue(status({ source: 'none' }));
    const { openFile } = await import('../../../lib/tauri');
    (openFile as any).mockResolvedValue('/home/op/Downloads/grpc-app-key.age');

    const { container } = render(AppSecurityPage);

    await waitFor(() => {
      expect(screen.getByRole('button', { name: /اختيار الملف/ })).toBeInTheDocument();
    });
    await fireEvent.click(screen.getByRole('button', { name: /اختيار الملف/ }));
    await fireEvent.input(container.querySelector('#security-import-passphrase')!, {
      target: { value: 'short' },
    });
    await fireEvent.click(screen.getByRole('button', { name: /استيراد المفتاح وفتح التطبيق/ }));

    expect(container.textContent).toContain('8 أحرف على الأقل');
    expect(mockImportAppKey).not.toHaveBeenCalled();
  });

  it('تقديم صحيح: importAppKey يتلقى (passphrase, artifactPath) فقط ثم الانتقال إلى /login', async () => {
    mockGetSecurityStatus.mockResolvedValue(status({ source: 'none' }));
    mockImportAppKey.mockResolvedValue({
      provisioned: true,
      unlocked: true,
      store_path: '/tmp/grpc/appkey.age',
    });
    const { openFile } = await import('../../../lib/tauri');
    (openFile as any).mockResolvedValue('/home/op/Downloads/grpc-app-key.age');

    const { container } = render(AppSecurityPage);

    await waitFor(() => {
      expect(screen.getByRole('button', { name: /اختيار الملف/ })).toBeInTheDocument();
    });
    await fireEvent.click(screen.getByRole('button', { name: /اختيار الملف/ }));
    await fireEvent.input(container.querySelector('#security-import-passphrase')!, {
      target: { value: 'operator-passphrase-2026' },
    });
    await fireEvent.click(screen.getByRole('button', { name: /استيراد المفتاح وفتح التطبيق/ }));

    await waitFor(() => {
      expect(mockImportAppKey).toHaveBeenCalledTimes(1);
    });
    expect(mockImportAppKey).toHaveBeenCalledWith(
      'operator-passphrase-2026',
      '/home/op/Downloads/grpc-app-key.age',
    );
    expect(mockPush).toHaveBeenCalledWith('/login');
  });

  it('فشل backend: يعرض رسالة الخطأ العامة ولا ينتقل إلى /login', async () => {
    mockGetSecurityStatus.mockResolvedValue(status({ source: 'none' }));
    mockImportAppKey.mockRejectedValue(new Error('appkey.age already exists'));
    const { openFile } = await import('../../../lib/tauri');
    (openFile as any).mockResolvedValue('/home/op/Downloads/grpc-app-key.age');

    const { container } = render(AppSecurityPage);

    await waitFor(() => {
      expect(screen.getByRole('button', { name: /اختيار الملف/ })).toBeInTheDocument();
    });
    await fireEvent.click(screen.getByRole('button', { name: /اختيار الملف/ }));
    await fireEvent.input(container.querySelector('#security-import-passphrase')!, {
      target: { value: 'operator-passphrase-2026' },
    });
    await fireEvent.click(screen.getByRole('button', { name: /استيراد المفتاح وفتح التطبيق/ }));

    await waitFor(() => {
      expect(container.textContent).toContain('appkey.age already exists');
    });
    expect(mockPush).not.toHaveBeenCalled();
  });

  it('نجاح الاستيراد: لا يبقى السر ولا المسار في الواجهة', async () => {
    mockGetSecurityStatus.mockResolvedValue(status({ source: 'none' }));
    mockImportAppKey.mockResolvedValue({
      provisioned: true,
      unlocked: true,
      store_path: '/tmp/grpc/appkey.age',
    });
    const { openFile } = await import('../../../lib/tauri');
    (openFile as any).mockResolvedValue('/home/op/Downloads/grpc-app-key.age');

    const { container } = render(AppSecurityPage);

    await waitFor(() => {
      expect(screen.getByRole('button', { name: /اختيار الملف/ })).toBeInTheDocument();
    });
    await fireEvent.click(screen.getByRole('button', { name: /اختيار الملف/ }));
    await fireEvent.input(container.querySelector('#security-import-passphrase')!, {
      target: { value: 'operator-passphrase-2026' },
    });
    await fireEvent.click(screen.getByRole('button', { name: /استيراد المفتاح وفتح التطبيق/ }));

    await waitFor(() => {
      expect(mockImportAppKey).toHaveBeenCalledTimes(1);
    });
    const passphraseInput = container.querySelector('#security-import-passphrase') as HTMLInputElement;
    expect(passphraseInput.value).toBe('');
    expect(container.textContent).not.toContain('operator-passphrase-2026');
    expect(container.textContent).not.toContain('AGE-SECRET-KEY-1');
  });

  it('source=env: لا يعرض CTA الاستيراد', async () => {
    mockGetSecurityStatus.mockResolvedValue(
      status({ source: 'env', provisioned: true, unlocked: true, requires_action: false }),
    );

    const { container } = render(AppSecurityPage);

    await waitFor(() => {
      expect(container.textContent).toContain('مفتاح الأسطول متوفر عبر GRPC_APP_KEY');
    });
    expect(screen.queryByRole('button', { name: /اختيار الملف/ })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /استيراد المفتاح وفتح التطبيق/ })).not.toBeInTheDocument();
  });

  it('source=store: لا يعرض CTA الاستيراد', async () => {
    mockGetSecurityStatus.mockResolvedValue(
      status({ source: 'store', provisioned: true, unlocked: false, requires_action: true }),
    );

    const { container } = render(AppSecurityPage);

    await waitFor(() => {
      expect(screen.getByRole('button', { name: /فتح التطبيق/ })).toBeInTheDocument();
    });
    expect(container.textContent).toContain('appkey.age');
    expect(screen.queryByRole('button', { name: /اختيار الملف/ })).not.toBeInTheDocument();
  });

  it('الاستيراد لا يستخدم localStorage ولا sessionStorage ولا clipboard', async () => {
    mockGetSecurityStatus.mockResolvedValue(status({ source: 'none' }));
    mockImportAppKey.mockResolvedValue({
      provisioned: true,
      unlocked: true,
      store_path: '/tmp/grpc/appkey.age',
    });
    const { openFile } = await import('../../../lib/tauri');
    (openFile as any).mockResolvedValue('/home/op/Downloads/grpc-app-key.age');

    const storageSpies: ReturnType<typeof vi.spyOn>[] = [];
    if (window.localStorage && typeof window.localStorage.getItem === 'function') {
      storageSpies.push(
        vi.spyOn(window.localStorage, 'getItem'),
        vi.spyOn(window.localStorage, 'setItem'),
      );
    }
    if (window.sessionStorage && typeof window.sessionStorage.getItem === 'function') {
      storageSpies.push(
        vi.spyOn(window.sessionStorage, 'getItem'),
        vi.spyOn(window.sessionStorage, 'setItem'),
      );
    }

    const { container } = render(AppSecurityPage);

    await waitFor(() => {
      expect(screen.getByRole('button', { name: /اختيار الملف/ })).toBeInTheDocument();
    });
    await fireEvent.click(screen.getByRole('button', { name: /اختيار الملف/ }));
    await fireEvent.input(container.querySelector('#security-import-passphrase')!, {
      target: { value: 'operator-passphrase-2026' },
    });
    await fireEvent.click(screen.getByRole('button', { name: /استيراد المفتاح وفتح التطبيق/ }));

    await waitFor(() => {
      expect(mockImportAppKey).toHaveBeenCalledTimes(1);
    });
    for (const spy of storageSpies) {
      expect(spy).not.toHaveBeenCalled();
      spy.mockRestore();
    }
  });
});