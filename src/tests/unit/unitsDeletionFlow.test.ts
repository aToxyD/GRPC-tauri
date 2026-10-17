/**
 * ADR-0064 — WILAYA UNIT deletion flow (UnitsPage).
 *
 * Focused frontend regression coverage for the existing `delete_unit`
 * integration. Deletion eligibility is owned by the backend (ADR-0064 §3:
 * deletion is NOT gated on EXPORTED state); the frontend only invokes the
 * existing `deleteUnit` contract.
 *
 * Scenarios:
 * - A: the row Delete action opens the confirmation dialog and does not call
 *      the backend until the explicit confirmation action is invoked;
 * - B: a successful confirmation calls `deleteUnit(unitId)`, closes the
 *      dialog, surfaces the success message, and reloads the unit list;
 * - C: a fail-closed backend rejection keeps the dialog open, surfaces the
 *      error, and leaves the displayed list untouched;
 * - D: concurrent confirmations issue exactly one backend deletion, and the
 *      in-flight guard is released once the request settles.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, fireEvent, screen, waitFor, within } from '@testing-library/svelte';
import UnitsPage from '../../pages/UnitsPage.svelte';

const mockExportUnitNodePackage = vi.fn();
const mockListUnits = vi.fn();
const mockGetSettings = vi.fn();
const mockCreateUnit = vi.fn();
const mockUpdateUnit = vi.fn();
const mockDeleteUnit = vi.fn();
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
  exportUnitNodePackage: (...args: any[]) => mockExportUnitNodePackage(...args),
  listUnits: (...args: any[]) => mockListUnits(...args),
  createUnit: (...args: any[]) => mockCreateUnit(...args),
  updateUnit: (...args: any[]) => mockUpdateUnit(...args),
  deleteUnit: (...args: any[]) => mockDeleteUnit(...args),
  getSettings: (...args: any[]) => mockGetSettings(...args),
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

const UNIT = {
  id: 'unit-1',
  code: 'U1',
  name: 'Unit 1',
  wilaya_code: '31',
  user_id: 'user-1',
  created_at: '2026-08-01T00:00:00Z',
};

async function renderUnits() {
  render(UnitsPage);
  await waitFor(() => expect(screen.getByText('Unit 1')).toBeInTheDocument());
}

async function openDeleteDialog() {
  // Before the dialog opens, the row Delete button is the only accessible
  // "حذف" button on the page.
  await fireEvent.click(screen.getByRole('button', { name: 'حذف' }));
  await waitFor(() =>
    expect(within(screen.getByRole('dialog')).getByText('تأكيد الحذف')).toBeInTheDocument()
  );
}

function confirmButton(): HTMLButtonElement {
  return within(screen.getByRole('dialog')).getByRole('button', { name: 'حذف' }) as HTMLButtonElement;
}

describe('WILAYA UNIT deletion flow (ADR-0064)', () => {
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
    mockListUnits.mockResolvedValue([UNIT]);
    mockDeleteUnit.mockResolvedValue(undefined);
  });

  it('A — confirmation safeguard: opening the dialog does not delete; confirming does', async () => {
    await renderUnits();
    await openDeleteDialog();

    // Opening the confirmation must not itself trigger a deletion.
    expect(mockDeleteUnit).not.toHaveBeenCalled();

    // The explicit confirmation action invokes the existing contract with the
    // unit id.
    await fireEvent.click(confirmButton());
    await waitFor(() => expect(mockDeleteUnit).toHaveBeenCalledWith('unit-1'));
    expect(mockDeleteUnit).toHaveBeenCalledTimes(1);
  });

  it('B — successful deletion closes the dialog, reports success, and reloads the list', async () => {
    await renderUnits();
    const initialListCalls = mockListUnits.mock.calls.length;

    await openDeleteDialog();
    await fireEvent.click(confirmButton());

    await waitFor(() => expect(mockDeleteUnit).toHaveBeenCalledWith('unit-1'));

    // The dialog closes only after a successful deletion.
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());

    // Success is surfaced to the user.
    expect(screen.getByText(/تم حذف الوحدة U1 بنجاح/)).toBeInTheDocument();

    // The unit list is reloaded.
    await waitFor(() =>
      expect(mockListUnits.mock.calls.length).toBeGreaterThan(initialListCalls)
    );
  });

  it('C — fail-closed rejection keeps the dialog open and the list untouched', async () => {
    mockDeleteUnit.mockRejectedValueOnce(
      new Error('لا يمكن حذف الوحدة: لا يمكن حذف وحدة تحتوي على بيانات مرتبطة')
    );

    await renderUnits();
    const initialListCalls = mockListUnits.mock.calls.length;

    await openDeleteDialog();
    await fireEvent.click(confirmButton());

    // The backend error is surfaced inside the still-open dialog.
    await waitFor(() =>
      expect(
        within(screen.getByRole('dialog')).getByText(/لا يمكن حذف الوحدة/)
      ).toBeInTheDocument()
    );
    expect(screen.getByRole('dialog')).toBeInTheDocument();

    // Failure is not treated as success: no success message, the unit remains
    // visible, and no list reload occurs.
    expect(screen.queryByText(/تم حذف الوحدة U1 بنجاح/)).toBeNull();
    expect(screen.getByText('Unit 1')).toBeInTheDocument();
    expect(mockListUnits.mock.calls.length).toBe(initialListCalls);
  });

  it('D — duplicate submission prevention and guard release', async () => {
    let rejectFirst!: (reason?: unknown) => void;
    const pending = new Promise<never>((_resolve, reject) => {
      rejectFirst = reject;
    });
    mockDeleteUnit.mockImplementationOnce(() => pending);

    await renderUnits();
    await openDeleteDialog();

    const confirm = confirmButton();

    // Two submissions while the first request is still pending. Native clicks
    // dispatch synchronously, before Svelte flushes the disabled attribute, so
    // the handler-level guard is what must prevent the second backend call.
    confirm.click();
    confirm.click();
    expect(mockDeleteUnit).toHaveBeenCalledTimes(1);

    // Settle the first request as a failure; the guard must be released.
    rejectFirst(new Error('لا يمكن حذف الوحدة: تعذّر الحذف'));
    await waitFor(() =>
      expect(within(screen.getByRole('dialog')).getByText(/لا يمكن حذف الوحدة/)).toBeInTheDocument()
    );
    await waitFor(() => expect(confirm.disabled).toBe(false));

    // A subsequent confirmation issues a fresh backend call (guard reset).
    confirm.click();
    await waitFor(() => expect(mockDeleteUnit).toHaveBeenCalledTimes(2));
  });
});
