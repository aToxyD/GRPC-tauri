/**
 * ADR-0064 D11 / D19.7 — WILAYA Units surface: EXPORTED state gating (UnitsPage).
 *
 * Owner decision (2026-10-20): keep both the Edit and Export buttons visible
 * on an EXPORTED unit, disable them, and explain why the actions are
 * unavailable. The backend refusals (`update_unit`, `export_unit_node_package`)
 * remain the authoritative guards; the UI state is a usability layer only.
 *
 * Scenarios:
 * - A: a non-exported unit keeps fully functional Edit and Export actions and
 *      renders no unavailability note;
 * - B: an exported unit keeps Edit and Export visible but disabled
 *      (`disabled` + `aria-disabled`), each carrying an accessible reason
 *      (`title` / `aria-label`) plus a visible explanation in the row, and
 *      the disabled actions cannot be triggered (no dialog opens, no
 *      `saveFile` / `exportUnitNodePackage` / `updateUnit` call occurs);
 * - C: in a mixed list only the exported row is gated;
 * - D: Delete remains available and functional on an exported unit
 *      (ADR-0064 §3: deletion is NOT gated on EXPORTED state).
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

const UNIT_EXPORTED = {
  id: 'unit-1',
  code: 'U1',
  name: 'Unit 1',
  wilaya_code: '31',
  user_id: 'user-1',
  created_at: '2026-08-01T00:00:00Z',
  is_exported: true,
};

const UNIT_FRESH = {
  id: 'unit-2',
  code: 'U2',
  name: 'Unit 2',
  wilaya_code: '31',
  user_id: 'user-2',
  created_at: '2026-08-02T00:00:00Z',
  is_exported: false,
};

const EXPORTED_ROW_NOTE = /تعديل وتصدير غير متاحين/;

async function renderUnits(units: unknown[], expectedName: string) {
  mockListUnits.mockResolvedValue(units);
  render(UnitsPage);
  await waitFor(() => expect(screen.getByText(expectedName)).toBeInTheDocument());
}

/** The rendered data rows, in list order. */
function dataRows(): HTMLElement[] {
  return screen.getAllByRole('row').filter((row) => row.querySelector('td') !== null);
}

/** Edit/Export buttons of a single row.
 *
 * Located by stable child anchors (the visible label span) rather than by the
 * accessible name, because a disabled button's explanation itself contains
 * both action words and would make name-based queries ambiguous.
 */
function editButton(row: HTMLElement): HTMLButtonElement {
  const label = within(row).getByText('تعديل', { selector: 'span' });
  return label.closest('button') as HTMLButtonElement;
}

function exportButton(row: HTMLElement): HTMLButtonElement {
  const label = within(row).getByText('تصدير', { selector: 'span' });
  return label.closest('button') as HTMLButtonElement;
}

describe('WILAYA Units surface — EXPORTED state gating (ADR-0064 D11 / D19.7)', () => {
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
    mockListUnits.mockResolvedValue([UNIT_EXPORTED]);
    mockSaveFile.mockResolvedValue('/tmp/U1_package.unit');
    mockExportUnitNodePackage.mockResolvedValue({ success: true, message: '' });
    mockDeleteUnit.mockResolvedValue(undefined);
  });

  it('A — non-exported unit: Edit and Export stay enabled and functional, no note', async () => {
    await renderUnits([UNIT_FRESH], 'Unit 2');
    // Wait for the fresh unit row to render.
    await waitFor(() => expect(screen.getByText('Unit 2')).toBeInTheDocument());

    const row = dataRows()[0];
    const edit = editButton(row);
    const exportBtn = exportButton(row);
    expect(edit.disabled).toBe(false);
    expect(edit.getAttribute('aria-disabled')).toBe('false');
    expect(exportBtn.disabled).toBe(false);
    expect(exportBtn.getAttribute('aria-disabled')).toBe('false');
    expect(screen.queryByText(EXPORTED_ROW_NOTE)).toBeNull();

    // Edit still opens the edit dialog.
    await fireEvent.click(edit);
    await waitFor(() =>
      expect(within(screen.getByRole('dialog')).getByText('تعديل الوحدة')).toBeInTheDocument()
    );

    // Export still runs the save-file → export flow.
    await fireEvent.click(exportBtn);
    await waitFor(() => expect(mockSaveFile).toHaveBeenCalledTimes(1));
    await waitFor(() =>
      expect(mockExportUnitNodePackage).toHaveBeenCalledWith('unit-2', '/tmp/U1_package.unit')
    );
  });

  it('B — exported unit: Edit and Export remain visible but disabled, explained, and untriggerable', async () => {
    await renderUnits([UNIT_EXPORTED], 'Unit 1');

    // Both actions remain VISIBLE (owner decision: do not hide the buttons).
    const row = dataRows()[0];
    const edit = editButton(row);
    const exportBtn = exportButton(row);
    expect(edit).toBeVisible();
    expect(exportBtn).toBeVisible();

    // Both are disabled with an accessible reason.
    expect(edit.disabled).toBe(true);
    expect(edit.getAttribute('aria-disabled')).toBe('true');
    expect(edit.getAttribute('title')).toMatch(/تم تصدير هوية الوحدة/);
    expect(edit.getAttribute('aria-label')).toMatch(/تم تصدير هوية الوحدة/);
    expect(exportBtn.disabled).toBe(true);
    expect(exportBtn.getAttribute('aria-disabled')).toBe('true');
    expect(exportBtn.getAttribute('title')).toMatch(/تم تصدير هوية الوحدة/);
    expect(exportBtn.getAttribute('aria-label')).toMatch(/تم تصدير هوية الوحدة/);

    // A visible explanation is rendered in the row.
    expect(screen.getByText(EXPORTED_ROW_NOTE)).toBeInTheDocument();

    // The disabled Edit action cannot be triggered: no dialog, no backend call.
    await fireEvent.click(edit);
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(mockUpdateUnit).not.toHaveBeenCalled();

    // The disabled Export action cannot be triggered: no file dialog, no export.
    await fireEvent.click(exportBtn);
    expect(mockSaveFile).not.toHaveBeenCalled();
    expect(mockExportUnitNodePackage).not.toHaveBeenCalled();
  });

  it('C — mixed list: only the exported row is gated', async () => {
    await renderUnits([UNIT_EXPORTED, UNIT_FRESH], 'Unit 1');
    await waitFor(() => expect(screen.getByText('Unit 2')).toBeInTheDocument());

    // The exported row's actions are disabled; the fresh row's are not.
    const rows = dataRows();
    expect(rows).toHaveLength(2);
    const exportedEdit = editButton(rows[0]);
    const exportedExport = exportButton(rows[0]);
    const freshEdit = editButton(rows[1]);
    const freshExport = exportButton(rows[1]);
    expect(exportedEdit.disabled).toBe(true);
    expect(exportedExport.disabled).toBe(true);
    expect(freshEdit.disabled).toBe(false);
    expect(freshExport.disabled).toBe(false);

    // Exactly one unavailability note, for the exported row.
    expect(screen.getAllByText(EXPORTED_ROW_NOTE)).toHaveLength(1);

    // The fresh row's Export is still functional.
    await fireEvent.click(freshExport);
    await waitFor(() => expect(mockSaveFile).toHaveBeenCalledTimes(1));
  });

  it('D — Delete remains available and functional on an exported unit', async () => {
    await renderUnits([UNIT_EXPORTED], 'Unit 1');

    const deleteBtn = screen.getByRole('button', { name: 'حذف' }) as HTMLButtonElement;
    expect(deleteBtn.disabled).toBe(false);

    await fireEvent.click(deleteBtn);
    await waitFor(() =>
      expect(within(screen.getByRole('dialog')).getByText('تأكيد الحذف')).toBeInTheDocument()
    );
    await fireEvent.click(
      within(screen.getByRole('dialog')).getByRole('button', { name: 'حذف' })
    );
    await waitFor(() => expect(mockDeleteUnit).toHaveBeenCalledWith('unit-1'));
  });
});
