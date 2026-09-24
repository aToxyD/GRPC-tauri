/**
 * WILAYA export path for the packaged-identity `.unit` package
 * (ADR-0044 / RFC §3.12) — UnitsPage.
 *
 * Verifies the frontend export contract:
 * - the export action invokes the expected `exportUnitNodePackage` API;
 * - the frontend does NOT attempt to generate, store, or display the UNIT
 *   private key (no crypto, no key-material UI);
 * - the backend remains the sole owner of embedding the packaged identity
 *   (`unit_certificate` + `unit_private_key`) into the `.unit` artifact.
 *
 * The embedded certificate/private-key contents are never observable on the
 * frontend by design; this test therefore checks only the API invocation and
 * the resulting UI state.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, fireEvent, screen, waitFor } from '@testing-library/svelte';
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

describe('WILAYA export path — packaged UNIT identity (ADR-0044)', () => {
  const unit = {
    id: 'unit-1',
    code: 'U1',
    name: 'Unit 1',
    wilaya_code: '31',
    user_id: 'user-1',
    created_at: '2026-08-01T00:00:00Z',
  };

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
    mockListUnits.mockResolvedValue([unit]);
    mockSaveFile.mockResolvedValue('/tmp/U1_package.unit');
    mockExportUnitNodePackage.mockResolvedValue({
      file_path: '/tmp/U1_package.unit',
      record_count: 1,
      success: true,
      message: '',
      file_hash: 'pkg-hash',
    });
  });

  it('export action invokes the expected `exportUnitNodePackage` API', async () => {
    render(UnitsPage);

    // Units load from the backend projection.
    await waitFor(() => {
      expect(screen.getByText('Unit 1')).toBeInTheDocument();
    });

    // Trigger the export action on the unit row.
    const exportButton = screen.getByRole('button', { name: /تصدير حزمة التكوين \(\.unit\)/ });
    await fireEvent.click(exportButton);

    // The frontend invokes the export API with the unit id and the chosen
    // file path — it does NOT build the package payload itself.
    await waitFor(() => {
      expect(mockSaveFile).toHaveBeenCalled();
      expect(mockExportUnitNodePackage).toHaveBeenCalledWith('unit-1', '/tmp/U1_package.unit');
    });
  });

  it('frontend does not generate, store, or display the UNIT private key', async () => {
    render(UnitsPage);

    await waitFor(() => {
      expect(screen.getByText('Unit 1')).toBeInTheDocument();
    });

    const exportButton = screen.getByRole('button', { name: /تصدير حزمة التكوين \(\.unit\)/ });
    await fireEvent.click(exportButton);

    // No key material is presented anywhere in the UI.
    expect(screen.queryByText(/المفتاح الخاص/)).toBeNull();
    expect(screen.queryByText(/unit_private_key/)).toBeNull();
    expect(screen.queryByText(/BEGIN ED25519|PRIVATE KEY/i)).toBeNull();

    // The export result carries no key material and no crypto is performed
    // on the frontend: the only export-related call is the package API.
    expect(mockExportUnitNodePackage).toHaveBeenCalledWith('unit-1', '/tmp/U1_package.unit');
    expect(mockExportUnitNodePackage.mock.calls.length).toBe(1);
  });
});