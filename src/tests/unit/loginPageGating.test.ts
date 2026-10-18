/**
 * Login visibility gating + first-run WILAYA/UNIT setup selector.
 *
 * These tests pin the PRESENTATION contract introduced for the login page:
 *
 *  1. Login controls stay hidden while the required initial status reads are
 *     still pending.
 *  2. Login controls stay hidden when any required status read rejects
 *     (fail closed — partial success is never initialization complete).
 *  3. Login controls stay hidden for `UNINITIALIZED`,
 *     `WAITING_FOR_ROOT_CERTIFICATE`, and `WILAYA_ACTIVE` on an unconfigured node.
 *  4. Login controls become visible for `ADMIN_PROVISIONED`, `READY`,
 *     `UNIT_ACTIVE`, or a successfully configured node.
 *  5. No transient flash of login controls during startup.
 *  6. The existing UNIT bootstrap/import entry stays reachable before auth.
 *  7. The WILAYA/UNIT selector is transient: switching tabs mutates no backend
 *     state and persists nothing.
 *  8. The selector is offered only for a genuinely uninitialized node.
 *  9. Starting provisioning (authoritative state change) hides the selector.
 * 10. A cancelled action that leaves the node uninitialized does not lock it.
 * 11. A partial/established node cannot use the selector to change node type.
 * 12. Setup-path isolation: WILAYA-only continuation states
 *     (`WAITING_FOR_ROOT_CERTIFICATE`, `ADMIN_PROVISIONED`, `READY`) never
 *     expose the UNIT `.unit` package-import entry; UNIT-only states
 *     (`UNIT_WAITING_FOR_CERTIFICATE`, `UNIT_ACTIVE`) never expose WILAYA-only
 *     setup UI; configured nodes expose no unrelated setup-entry UI.
 * 13. Rendering exclusivity: the WILAYA first-administrator form and the UNIT
 *     `.unit` entry are never visible simultaneously. On an unconfigured node
 *     the ambiguous `WILAYA_ACTIVE` state exposes the selector so the operator
 *     chooses ONE continuation; the same-session selection survives the
 *     `UNINITIALIZED` → `WILAYA_ACTIVE` transition, and after a restart the
 *     selector lets the operator choose again. A stale session hint only
 *     preselects — it never removes a continuation or overrides an
 *     unambiguous identity state.
 *
 * This is a presentation test only. The backend remains the authority for every
 * authentication and provisioning decision; nothing here re-implements authz.
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
const mockSaveFile = vi.fn();
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

/** A promise that never settles — models a status read still in flight. */
const NEVER = new Promise<never>(() => {});

const SELECTOR_GROUP = 'نوع العقدة';
const PASSWORD_PLACEHOLDER = 'أدخل كلمة المرور';

function expectLoginControlsHidden(): void {
  expect(screen.queryByPlaceholderText(PASSWORD_PLACEHOLDER)).toBeNull();
  expect(screen.queryByRole('button', { name: 'تسجيل الدخول' })).toBeNull();
  expect(screen.queryByRole('tab', { name: 'كلمة المرور' })).toBeNull();
}

function expectLoginControlsVisible(): void {
  expect(screen.getByPlaceholderText(PASSWORD_PLACEHOLDER)).toBeInTheDocument();
  expect(screen.getByRole('button', { name: 'تسجيل الدخول' })).toBeInTheDocument();
  expect(screen.getByRole('tab', { name: 'كلمة المرور' })).toBeInTheDocument();
}

describe('LoginPage visibility gating (statusLoaded / loginEligible)', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    // Default: an eligible, configured WILAYA node. Individual tests override.
    mockIsConfigured.mockResolvedValue(true);
    mockGetSettings.mockResolvedValue({ configured: true, node_type: 'WILAYA' });
    mockGetIdentityStatus.mockResolvedValue('READY');
    mockGetSecurityStatus.mockResolvedValue({ requires_action: false, source: 'env' });
  });

  it('keeps login controls hidden while the initial status reads are pending', async () => {
    mockIsConfigured.mockReturnValue(NEVER);
    mockGetSettings.mockReturnValue(NEVER);
    mockGetIdentityStatus.mockReturnValue(NEVER);
    mockGetSecurityStatus.mockReturnValue(NEVER);

    render(LoginPage);
    await Promise.resolve();

    expectLoginControlsHidden();
  });

  it('keeps login controls hidden when a required status read rejects (fail closed)', async () => {
    mockIsConfigured.mockResolvedValue(true);
    mockGetSettings.mockResolvedValue({ configured: true, node_type: 'WILAYA' });
    mockGetSecurityStatus.mockResolvedValue({ requires_action: false, source: 'env' });
    mockGetIdentityStatus.mockRejectedValue(new Error('ipc down'));

    render(LoginPage);

    await screen.findByText(/تعذر قراءة حالة الهوية/);
    expectLoginControlsHidden();
  });

  it('keeps login controls hidden when only the settings read rejects', async () => {
    mockGetSettings.mockRejectedValue(new Error('ipc down'));

    render(LoginPage);

    await waitFor(() => {
      expect(screen.getByText(/تعذر قراءة حالة تهيئة العقدة/)).toBeInTheDocument();
    });
    expectLoginControlsHidden();
  });

  it.each(['UNINITIALIZED', 'WAITING_FOR_ROOT_CERTIFICATE', 'WILAYA_ACTIVE'] as const)(
    'hides login controls for %s on an unconfigured node',
    async (state) => {
      mockIsConfigured.mockResolvedValue(false);
      mockGetSettings.mockResolvedValue({ configured: false, node_type: 'UNCONFIGURED' });
      mockGetIdentityStatus.mockResolvedValue(state);

      render(LoginPage);

      // Wait for onMount to settle (the identity read is the last one), then
      // assert the gate. The selector is intentionally absent for the partial
      // states, so this wait must not depend on it.
      await screen.findByTestId('login-page');
      await waitFor(() => expect(mockGetIdentityStatus).toHaveBeenCalled());
      expectLoginControlsHidden();
    }
  );

  it.each([
    ['ADMIN_PROVISIONED', false],
    ['READY', false],
    ['UNIT_ACTIVE', false],
    ['UNINITIALIZED', true],
  ] as const)(
    'shows login controls for identity %s (configured=%s)',
    async (state, configured) => {
      mockIsConfigured.mockResolvedValue(configured);
      mockGetSettings.mockResolvedValue(
        configured ? { configured: true, node_type: 'WILAYA' } : { configured: false, node_type: 'UNCONFIGURED' }
      );
      mockGetIdentityStatus.mockResolvedValue(state);

      render(LoginPage);

      await waitFor(() => expectLoginControlsVisible());
    }
  );

  it('does not flash login controls before the status reads resolve', async () => {
    let resolveIdentity!: (value: string) => void;
    const pendingIdentity = new Promise<string>((resolve) => {
      resolveIdentity = resolve;
    });
    mockGetIdentityStatus.mockReturnValue(pendingIdentity);

    render(LoginPage);
    await Promise.resolve();

    expectLoginControlsHidden();

    resolveIdentity('READY');
    await waitFor(() => expectLoginControlsVisible());
  });
});

describe('LoginPage first-run WILAYA/UNIT setup selector', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    sessionStorage.clear();
    mockIsConfigured.mockResolvedValue(false);
    mockGetSettings.mockResolvedValue({ configured: false, node_type: 'UNCONFIGURED' });
    mockGetIdentityStatus.mockResolvedValue('UNINITIALIZED');
    mockGetSecurityStatus.mockResolvedValue({ requires_action: false, source: 'env' });
    mockSaveFile.mockResolvedValue('/tmp/grpc-wilaya-request.json');
    mockBeginWilayaProvision.mockResolvedValue({ subject_type: 'WILAYA' });
  });

  it('offers the selector on a genuinely uninitialized node', async () => {
    render(LoginPage);

    expect(await screen.findByRole('group', { name: SELECTOR_GROUP })).toBeInTheDocument();
    expectLoginControlsHidden();
  });

  it.each(['WAITING_FOR_ROOT_CERTIFICATE', 'UNIT_WAITING_FOR_CERTIFICATE'])(
    'does not offer the selector to the unambiguous continuation state %s',
    async (state) => {
      mockGetIdentityStatus.mockResolvedValue(state);
      mockGetSettings.mockResolvedValue({ configured: false, node_type: 'UNCONFIGURED' });

      render(LoginPage);
      await waitFor(() => expect(mockGetIdentityStatus).toHaveBeenCalled());

      expect(screen.queryByRole('group', { name: SELECTOR_GROUP })).toBeNull();
    }
  );

  it('keeps the existing UNIT bootstrap/import entry reachable before auth', async () => {
    render(LoginPage);

    await screen.findByRole('group', { name: SELECTOR_GROUP });
    // Default tab is UNIT → the existing ADR-0045 anchor-first entry is present.
    expect(
      screen.getByRole('button', { name: /الخطوة 1: تثبيت شهادة WILAYA \(مرساة الثقة\)/ })
    ).toBeInTheDocument();
    expect(
      screen.getByRole('button', { name: /استيراد حزمة التكوين \(\.unit\)/ })
    ).toBeInTheDocument();
    // ...and the WILAYA-only setup UI is NOT drawn at the same time.
    expect(screen.queryByRole('button', { name: /تصدير طلب توقيع العقدة/ })).toBeNull();
    expect(
      screen.queryByRole('button', { name: /إصدار المفتاح الإداري الأول/ })
    ).toBeNull();
    expectLoginControlsHidden();
  });

  it('switching tabs mutates no backend state (transient presentation only)', async () => {
    render(LoginPage);
    await screen.findByRole('group', { name: SELECTOR_GROUP });

    await fireEvent.click(screen.getByRole('button', { name: 'ولاية (WILAYA)' }));
    expect(screen.getByRole('button', { name: /تصدير طلب توقيع العقدة/ })).toBeInTheDocument();

    await fireEvent.click(screen.getByRole('button', { name: 'وحدة (UNIT)' }));
    expect(
      screen.getByRole('button', { name: /استيراد حزمة التكوين \(\.unit\)/ })
    ).toBeInTheDocument();

    // A bare tab click performs no provisioning and persists nothing.
    expect(mockSaveFile).not.toHaveBeenCalled();
    expect(mockOpenFile).not.toHaveBeenCalled();
    expect(mockBeginWilayaProvision).not.toHaveBeenCalled();
    expect(mockBeginUnitProvision).not.toHaveBeenCalled();
    expect(mockImportUnitNodePackage).not.toHaveBeenCalled();
    expect(mockInstallWilayaCertificate).not.toHaveBeenCalled();
    expect(mockIssueFirstAdminKey).not.toHaveBeenCalled();
    expect(screen.getByRole('group', { name: SELECTOR_GROUP })).toBeInTheDocument();
  });

  it('starting provisioning that reaches WAITING_FOR_ROOT_CERTIFICATE hides the selector and drops stale content', async () => {
    mockGetIdentityStatus
      .mockResolvedValueOnce('UNINITIALIZED')
      .mockResolvedValue('WAITING_FOR_ROOT_CERTIFICATE');

    render(LoginPage);
    await screen.findByRole('group', { name: SELECTOR_GROUP });

    await fireEvent.click(screen.getByRole('button', { name: 'ولاية (WILAYA)' }));
    await fireEvent.click(await screen.findByRole('button', { name: /تصدير طلب توقيع العقدة/ }));

    await waitFor(() => expect(mockBeginWilayaProvision).toHaveBeenCalledWith('/tmp/grpc-wilaya-request.json'));
    await waitFor(() =>
      expect(screen.queryByRole('group', { name: SELECTOR_GROUP })).toBeNull()
    );
    // The CSR step is replaced by the finalize step — no stale setup content.
    expect(screen.queryByRole('button', { name: /تصدير طلب توقيع العقدة/ })).toBeNull();
    expect(
      screen.getByRole('button', { name: /استيراد الشهادة الموقعة من المرجع/ })
    ).toBeInTheDocument();
  });

  it('a cancelled action that leaves the node uninitialized does not lock the selector', async () => {
    mockSaveFile.mockResolvedValue(null);

    render(LoginPage);
    await screen.findByRole('group', { name: SELECTOR_GROUP });

    await fireEvent.click(screen.getByRole('button', { name: 'ولاية (WILAYA)' }));
    await fireEvent.click(await screen.findByRole('button', { name: /تصدير طلب توقيع العقدة/ }));

    await waitFor(() => expect(mockSaveFile).toHaveBeenCalled());
    expect(mockBeginWilayaProvision).not.toHaveBeenCalled();
    // No authoritative change → the selector remains available.
    expect(screen.getByRole('group', { name: SELECTOR_GROUP })).toBeInTheDocument();
  });

  it('a configured node never offers the selector', async () => {
    mockIsConfigured.mockResolvedValue(true);
    mockGetSettings.mockResolvedValue({ configured: true, node_type: 'UNIT' });
    mockGetIdentityStatus.mockResolvedValue('UNIT_ACTIVE');

    render(LoginPage);
    await waitFor(() => expectLoginControlsVisible());

    expect(screen.queryByRole('group', { name: SELECTOR_GROUP })).toBeNull();
  });

  it('hides the UNIT package-import entry in WAITING_FOR_ROOT_CERTIFICATE and keeps the WILAYA continuation', async () => {
    mockGetIdentityStatus.mockResolvedValue('WAITING_FOR_ROOT_CERTIFICATE');

    render(LoginPage);

    // WILAYA-only continuation (import the Root-signed certificate) is present.
    expect(
      await screen.findByRole('button', { name: /استيراد الشهادة الموقعة من المرجع/ })
    ).toBeInTheDocument();
    // The UNIT entry — both the trust-anchor step and the `.unit` import — is
    // withheld because it belongs to the other path.
    expect(
      screen.queryByRole('button', { name: /استيراد حزمة التكوين \(\.unit\)/ })
    ).toBeNull();
    expect(
      screen.queryByRole('button', { name: /الخطوة 1: تثبيت شهادة WILAYA \(مرساة الثقة\)/ })
    ).toBeNull();
    expectLoginControlsHidden();
  });

  it.each(['ADMIN_PROVISIONED', 'READY'])(
    'hides the UNIT package-import entry for the WILAYA-only state %s even when configured === false',
    async (state) => {
      // Explicitly unconfigured: the identity projection alone must withhold it.
      mockIsConfigured.mockResolvedValue(false);
      mockGetSettings.mockResolvedValue({ configured: false, node_type: 'UNCONFIGURED' });
      mockGetIdentityStatus.mockResolvedValue(state);

      render(LoginPage);
      await waitFor(() => expectLoginControlsVisible());

      // Neither the setup selector nor the `.unit` import entry is exposed, and
      // a stale hint cannot re-enable them.
      expect(screen.queryByRole('group', { name: SELECTOR_GROUP })).toBeNull();
      expect(
        screen.queryByRole('button', { name: /استيراد حزمة التكوين \(\.unit\)/ })
      ).toBeNull();
      expect(
        screen.queryByRole('button', { name: /الخطوة 1: تثبيت شهادة WILAYA \(مرساة الثقة\)/ })
      ).toBeNull();
      // No WILAYA first-admin-key form either: that continuation is only valid
      // from `WILAYA_ACTIVE`.
      expect(
        screen.queryByRole('button', { name: /إصدار المفتاح الإداري الأول/ })
      ).toBeNull();
    }
  );

  it('hides WILAYA setup UI for UNIT_WAITING_FOR_CERTIFICATE and preserves the UNIT continuation', async () => {
    mockIsConfigured.mockResolvedValue(true);
    mockGetSettings.mockResolvedValue({ configured: true, node_type: 'UNIT' });
    mockGetIdentityStatus.mockResolvedValue('UNIT_WAITING_FOR_CERTIFICATE');

    render(LoginPage);

    // The legitimate UNIT-only continuation is preserved.
    expect(
      await screen.findByRole('button', { name: /الخطوة 2: استيراد شهادة الوحدة الموقعة/ })
    ).toBeInTheDocument();
    // WILAYA-only setup UI (CSR export / Root-cert import / first-admin-key) is
    // withheld, and the UNIT package-import entry is not re-offered.
    expect(screen.queryByRole('button', { name: /تصدير طلب توقيع العقدة/ })).toBeNull();
    expect(
      screen.queryByRole('button', { name: /استيراد الشهادة الموقعة من المرجع/ })
    ).toBeNull();
    expect(
      screen.queryByRole('button', { name: /إصدار المفتاح الإداري الأول/ })
    ).toBeNull();
    expect(
      screen.queryByRole('button', { name: /استيراد حزمة التكوين \(\.unit\)/ })
    ).toBeNull();
    // Existing login behavior for a configured UNIT node is preserved.
    expectLoginControlsVisible();
  });

  it('hides WILAYA setup UI for UNIT_ACTIVE and preserves the UNIT/login behavior', async () => {
    mockIsConfigured.mockResolvedValue(true);
    mockGetSettings.mockResolvedValue({ configured: true, node_type: 'UNIT' });
    mockGetIdentityStatus.mockResolvedValue('UNIT_ACTIVE');

    render(LoginPage);
    await waitFor(() => expectLoginControlsVisible());

    // The UNIT success continuation is preserved.
    expect(
      screen.getByText('هوية الوحدة مفعلة — يمكنك تسجيل الدخول.', { selector: 'p' })
    ).toBeInTheDocument();
    // WILAYA-only setup UI is withheld.
    expect(screen.queryByRole('button', { name: /تصدير طلب توقيع العقدة/ })).toBeNull();
    expect(
      screen.queryByRole('button', { name: /إصدار المفتاح الإداري الأول/ })
    ).toBeNull();
  });

  it.each([
    ['WILAYA', 'READY'],
    ['UNIT', 'UNIT_ACTIVE'],
  ])('shows no unrelated setup-entry UI on a configured %s node', async (nodeType, state) => {
    mockIsConfigured.mockResolvedValue(true);
    mockGetSettings.mockResolvedValue({ configured: true, node_type: nodeType });
    mockGetIdentityStatus.mockResolvedValue(state);

    render(LoginPage);
    await waitFor(() => expectLoginControlsVisible());

    expect(screen.queryByRole('group', { name: SELECTOR_GROUP })).toBeNull();
    expect(
      screen.queryByRole('button', { name: /استيراد حزمة التكوين \(\.unit\)/ })
    ).toBeNull();
    expect(screen.queryByRole('button', { name: /تصدير طلب توقيع العقدة/ })).toBeNull();
    expect(
      screen.queryByRole('button', { name: /إصدار المفتاح الإداري الأول/ })
    ).toBeNull();
  });

  it('keeps the WILAYA continuation in WAITING_FOR_ROOT_CERTIFICATE despite a stale UNIT hint', async () => {
    // A transient hint must never override a definitive identity state.
    sessionStorage.setItem('grpc_setup_node_type', 'UNIT');
    mockGetIdentityStatus.mockResolvedValue('WAITING_FOR_ROOT_CERTIFICATE');

    render(LoginPage);

    expect(
      await screen.findByRole('button', { name: /استيراد الشهادة الموقعة من المرجع/ })
    ).toBeInTheDocument();
    expect(
      screen.queryByRole('button', { name: /استيراد حزمة التكوين \(\.unit\)/ })
    ).toBeNull();
    sessionStorage.removeItem('grpc_setup_node_type');
  });

  it('renders only the UNIT continuation when UNIT is selected on a fresh node', async () => {
    render(LoginPage);
    await screen.findByRole('group', { name: SELECTOR_GROUP });

    // Default selection is UNIT → the UNIT entry, and nothing WILAYA-only.
    expect(
      screen.getByRole('button', { name: /استيراد حزمة التكوين \(\.unit\)/ })
    ).toBeInTheDocument();
    expect(
      screen.getByRole('button', { name: /الخطوة 1: تثبيت شهادة WILAYA \(مرساة الثقة\)/ })
    ).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /تصدير طلب توقيع العقدة/ })).toBeNull();
    expect(
      screen.queryByRole('button', { name: /إصدار المفتاح الإداري الأول/ })
    ).toBeNull();
  });

  it('preserves the UNIT selection across the same-session UNINITIALIZED → WILAYA_ACTIVE transition (no WILAYA form)', async () => {
    // Fresh UNIT provisioning: the operator installs the WILAYA trust anchor
    // from the UNIT tab. The state moves to the ambiguous WILAYA_ACTIVE, but the
    // selection must NOT switch to WILAYA — so the WILAYA first-admin form never
    // appears and the UNIT `.unit` import stays the active continuation.
    mockGetIdentityStatus
      .mockResolvedValueOnce('UNINITIALIZED')
      .mockResolvedValue('WILAYA_ACTIVE');
    mockOpenFile.mockResolvedValue('/tmp/wilaya-cert.json');
    mockInstallWilayaCertificate.mockResolvedValue({ Installed: 'ok' });

    render(LoginPage);
    await screen.findByRole('group', { name: SELECTOR_GROUP });

    await fireEvent.click(
      screen.getByRole('button', { name: /الخطوة 1: تثبيت شهادة WILAYA \(مرساة الثقة\)/ })
    );

    expect(
      await screen.findByRole('button', { name: /استيراد حزمة التكوين \(\.unit\)/ })
    ).toBeInTheDocument();
    expect(
      screen.queryByRole('button', { name: /إصدار المفتاح الإداري الأول/ })
    ).toBeNull();
    // The ambiguous state keeps the selector so the other continuation is still
    // reachable — but only one is drawn at a time.
    expect(screen.getByRole('group', { name: SELECTOR_GROUP })).toBeInTheDocument();
  });

  it('preserves the WILAYA selection across the same-session finalize transition (no UNIT entry)', async () => {
    // WILAYA provisioning: select WILAYA, export the CSR, import the Root-signed
    // certificate. The CSR step must give way to the first-admin form without
    // ever exposing the UNIT entry.
    mockGetIdentityStatus
      .mockResolvedValueOnce('UNINITIALIZED')
      .mockResolvedValueOnce('WAITING_FOR_ROOT_CERTIFICATE')
      .mockResolvedValue('WILAYA_ACTIVE');
    mockSaveFile.mockResolvedValue('/tmp/wilaya-request.json');
    mockBeginWilayaProvision.mockResolvedValue({ subject_type: 'WILAYA' });
    mockOpenFile.mockResolvedValue('/tmp/wilaya-signed.json');
    mockFinalizeWilayaProvision.mockResolvedValue({ Provisioned: 'ok' });

    render(LoginPage);
    await screen.findByRole('group', { name: SELECTOR_GROUP });
    await fireEvent.click(screen.getByRole('button', { name: 'ولاية (WILAYA)' }));

    await fireEvent.click(
      await screen.findByRole('button', { name: /تصدير طلب توقيع العقدة/ })
    );

    // WAITING_FOR_ROOT_CERTIFICATE: WILAYA continuation only, selector gone.
    expect(
      await screen.findByRole('button', { name: /استيراد الشهادة الموقعة من المرجع/ })
    ).toBeInTheDocument();
    expect(screen.queryByRole('group', { name: SELECTOR_GROUP })).toBeNull();
    expect(
      screen.queryByRole('button', { name: /استيراد حزمة التكوين \(\.unit\)/ })
    ).toBeNull();

    await fireEvent.click(
      screen.getByRole('button', { name: /استيراد الشهادة الموقعة من المرجع/ })
    );

    // WILAYA_ACTIVE: first-admin form only; the UNIT entry is never drawn.
    expect(
      await screen.findByRole('button', { name: /إصدار المفتاح الإداري الأول/ })
    ).toBeInTheDocument();
    expect(
      screen.queryByRole('button', { name: /استيراد حزمة التكوين \(\.unit\)/ })
    ).toBeNull();
  });

  it('offers exactly one continuation in ambiguous WILAYA_ACTIVE after a restart (no hint) and switches on selection', async () => {
    mockGetIdentityStatus.mockResolvedValue('WILAYA_ACTIVE');
    mockIsConfigured.mockResolvedValue(false);
    mockGetSettings.mockResolvedValue({ configured: false, node_type: 'UNCONFIGURED' });

    // No reliable persisted selection (app restart): the selector is exposed so
    // the operator can explicitly choose either continuation.
    sessionStorage.clear();
    render(LoginPage);
    await screen.findByRole('group', { name: SELECTOR_GROUP });

    // Default selection (UNIT) draws the UNIT entry only.
    expect(
      screen.getByRole('button', { name: /استيراد حزمة التكوين \(\.unit\)/ })
    ).toBeInTheDocument();
    expect(
      screen.queryByRole('button', { name: /إصدار المفتاح الإداري الأول/ })
    ).toBeNull();

    // Selecting WILAYA draws the first-admin form only.
    await fireEvent.click(screen.getByRole('button', { name: 'ولاية (WILAYA)' }));
    expect(
      screen.getByRole('button', { name: /إصدار المفتاح الإداري الأول/ })
    ).toBeInTheDocument();
    expect(
      screen.queryByRole('button', { name: /استيراد حزمة التكوين \(\.unit\)/ })
    ).toBeNull();

    // Selecting UNIT again draws the UNIT entry only.
    await fireEvent.click(screen.getByRole('button', { name: 'وحدة (UNIT)' }));
    expect(
      screen.getByRole('button', { name: /استيراد حزمة التكوين \(\.unit\)/ })
    ).toBeInTheDocument();
    expect(
      screen.queryByRole('button', { name: /إصدار المفتاح الإداري الأول/ })
    ).toBeNull();
  });

  it('a session hint preselects but never removes either continuation in ambiguous WILAYA_ACTIVE', async () => {
    mockGetIdentityStatus.mockResolvedValue('WILAYA_ACTIVE');
    mockIsConfigured.mockResolvedValue(false);
    mockGetSettings.mockResolvedValue({ configured: false, node_type: 'UNCONFIGURED' });

    for (const hint of ['WILAYA', 'UNIT'] as const) {
      sessionStorage.setItem('grpc_setup_node_type', hint);
      const { unmount } = render(LoginPage);
      await screen.findByRole('group', { name: SELECTOR_GROUP });

      // The hint only preselects: the selector is present (so both
      // continuations stay reachable) and exactly ONE is drawn at a time.
      const wilayaForm = screen.queryByRole('button', {
        name: /إصدار المفتاح الإداري الأول/,
      });
      const unitEntry = screen.queryByRole('button', {
        name: /استيراد حزمة التكوين \(\.unit\)/,
      });
      expect([wilayaForm, unitEntry].filter(Boolean)).toHaveLength(1);

      unmount();
    }
    sessionStorage.removeItem('grpc_setup_node_type');
  });

  it('restores a deliberate same-session WILAYA selection across a remount (hint round-trip)', async () => {
    mockGetIdentityStatus.mockResolvedValue('WILAYA_ACTIVE');
    mockIsConfigured.mockResolvedValue(false);
    mockGetSettings.mockResolvedValue({ configured: false, node_type: 'UNCONFIGURED' });

    const first = render(LoginPage);
    await screen.findByRole('group', { name: SELECTOR_GROUP });
    await fireEvent.click(screen.getByRole('button', { name: 'ولاية (WILAYA)' }));
    expect(
      screen.getByRole('button', { name: /إصدار المفتاح الإداري الأول/ })
    ).toBeInTheDocument();
    first.unmount();

    // Remount within the same webview session (reload): the hint restores the
    // WILAYA continuation and the UNIT entry is not drawn.
    render(LoginPage);
    expect(
      await screen.findByRole('button', { name: /إصدار المفتاح الإداري الأول/ })
    ).toBeInTheDocument();
    expect(
      screen.queryByRole('button', { name: /استيراد حزمة التكوين \(\.unit\)/ })
    ).toBeNull();
    sessionStorage.removeItem('grpc_setup_node_type');
  });

  it('does not lock the selector on a failed command that leaves the node uninitialized', async () => {
    // The command rejects but the authoritative projection is unchanged
    // (`getIdentityStatus` keeps resolving `UNINITIALIZED`), so the selector must
    // remain — an error alone is not an authoritative state change.
    mockBeginWilayaProvision.mockRejectedValue(new Error('CSR write rejected'));

    render(LoginPage);
    await screen.findByRole('group', { name: SELECTOR_GROUP });

    await fireEvent.click(screen.getByRole('button', { name: 'ولاية (WILAYA)' }));
    await fireEvent.click(await screen.findByRole('button', { name: /تصدير طلب توقيع العقدة/ }));

    await waitFor(() => expect(mockBeginWilayaProvision).toHaveBeenCalled());
    expect(await screen.findByText(/CSR write rejected/)).toBeInTheDocument();
    expect(screen.getByRole('group', { name: SELECTOR_GROUP })).toBeInTheDocument();
  });

  it('re-reads authoritative state after a failed provisioning command (no stale selector)', async () => {
    // The command rejects AFTER the backend persisted partial state: the node
    // key is written before the CSR file (`begin_wilaya_provision`), so the
    // identity projection has already moved off `UNINITIALIZED`.
    mockGetIdentityStatus
      .mockResolvedValueOnce('UNINITIALIZED')
      .mockResolvedValue('WAITING_FOR_ROOT_CERTIFICATE');
    mockBeginWilayaProvision.mockRejectedValue(new Error('CSR write failed'));

    render(LoginPage);
    await screen.findByRole('group', { name: SELECTOR_GROUP });

    await fireEvent.click(screen.getByRole('button', { name: 'ولاية (WILAYA)' }));
    await fireEvent.click(await screen.findByRole('button', { name: /تصدير طلب توقيع العقدة/ }));

    // The selector (UNINITIALIZED-only) is dropped and the WILAYA continuation
    // is shown — derived from the re-read projection, not the stale one.
    await waitFor(() =>
      expect(screen.queryByRole('group', { name: SELECTOR_GROUP })).toBeNull()
    );
    expect(
      screen.getByRole('button', { name: /استيراد الشهادة الموقعة من المرجع/ })
    ).toBeInTheDocument();
    // The original operation error is preserved, not masked by the refresh.
    expect(screen.getByText(/CSR write failed/)).toBeInTheDocument();
  });

  it('stays fail-closed and preserves the operation error when the post-failure refresh fails', async () => {
    mockGetIdentityStatus
      .mockResolvedValueOnce('UNINITIALIZED')
      .mockRejectedValue(new Error('ipc down'));
    mockBeginWilayaProvision.mockRejectedValue(new Error('CSR write failed'));

    render(LoginPage);
    await screen.findByRole('group', { name: SELECTOR_GROUP });

    await fireEvent.click(screen.getByRole('button', { name: 'ولاية (WILAYA)' }));
    await fireEvent.click(await screen.findByRole('button', { name: /تصدير طلب توقيع العقدة/ }));

    // No projection could be confirmed: the selector and login controls are
    // withheld (fail closed), and the ORIGINAL error remains with the
    // uncertainty appended rather than replaced.
    await waitFor(() =>
      expect(screen.queryByRole('group', { name: SELECTOR_GROUP })).toBeNull()
    );
    expectLoginControlsHidden();
    expect(screen.getByText(/CSR write failed/)).toBeInTheDocument();
    expect(screen.getByText(/تعذر تأكيد حالة العقدة بعد الفشل/)).toBeInTheDocument();
  });
});
