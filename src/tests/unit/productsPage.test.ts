/**
 * SEC-087 Phase 6D — ProductsPage create flow: unit/TVA configuration.
 *
 * Verifies the authorized WILAYA create path sends the four backend
 * configuration codes (`purchase_unit`, `consumption_unit`,
 * `conversion_factor`, `tva_classification`) through `createProduct`, and that
 * the read projection surfaces them in the table. Presentation only — the
 * canonical codes (UnitMeasure 1..=10, TvaClassification 0..=2) come from the
 * backend contract (ADR-0057 / ADR-0058); the update path must NOT carry them.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, fireEvent, screen, waitFor } from '@testing-library/svelte';
import ProductsPage from '../../pages/ProductsPage.svelte';

const mockListProducts = vi.fn();
const mockCreateProduct = vi.fn();
const mockUpdateProduct = vi.fn();
const mockDeleteProduct = vi.fn();
const mockGetSettings = vi.fn();
const mockExportProductsExcel = vi.fn();
const mockExportProductsPackage = vi.fn();
const mockImportProductsPackage = vi.fn();
const mockSaveFile = vi.fn();
const mockOpenFile = vi.fn();
const mockShowAsk = vi.fn();

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
  openFile: (...args: any[]) => mockOpenFile(...args),
  saveFile: (...args: any[]) => mockSaveFile(...args),
  showAsk: (...args: any[]) => mockShowAsk(...args),
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
  listProducts: (...args: any[]) => mockListProducts(...args),
  createProduct: (...args: any[]) => mockCreateProduct(...args),
  updateProduct: (...args: any[]) => mockUpdateProduct(...args),
  deleteProduct: (...args: any[]) => mockDeleteProduct(...args),
  getSettings: (...args: any[]) => mockGetSettings(...args),
  exportProductsExcel: (...args: any[]) => mockExportProductsExcel(...args),
  exportProductsPackage: (...args: any[]) => mockExportProductsPackage(...args),
  importProductsPackage: (...args: any[]) => mockImportProductsPackage(...args),
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

const configuredProduct = {
  id: 'p1',
  name: 'دقيق',
  base_price: 250,
  year: 2026,
  created_at: '2026-01-01T00:00:00Z',
  purchase_unit: 1,
  consumption_unit: 3,
  conversion_factor: 4,
  tva_classification: 1,
};

const settings = {
  node_type: 'WILAYA',
  wilaya_code: '31',
  wilaya_name: 'وهران',
  current_year: 2026,
};

describe('ProductsPage — create flow unit/TVA config (SEC-087 Phase 6D)', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockListProducts.mockResolvedValue([configuredProduct]);
    mockGetSettings.mockResolvedValue(settings);
    mockCreateProduct.mockResolvedValue('p2');
    mockUpdateProduct.mockResolvedValue(undefined);
    mockDeleteProduct.mockResolvedValue(undefined);
    mockShowAsk.mockResolvedValue(false);
  });

  it('renders the read projection with unit and TVA codes', async () => {
    render(ProductsPage);

    await waitFor(() => {
      expect(screen.getByText('دقيق')).toBeInTheDocument();
    });

    // SEC-087 Phase 6D — F-02 projection: the table surfaces the config in
    // three dedicated columns (purchase unit / consumption unit / factor).
    expect(screen.getByText('كلغ')).toBeInTheDocument();
    expect(screen.getByText('دلو')).toBeInTheDocument();
    expect(screen.getByText('×4')).toBeInTheDocument();
    expect(screen.getByText('9 %')).toBeInTheDocument();
  });

  it('sends the four configuration codes on create', async () => {
    render(ProductsPage);
    await waitFor(() => expect(mockListProducts).toHaveBeenCalled());

    await fireEvent.click(screen.getByRole('button', { name: 'إضافة منتج' }));

    await waitFor(() => {
      expect(screen.getByText('منتج جديد')).toBeInTheDocument();
    });

    await fireEvent.input(screen.getByLabelText(/الاسم/), { target: { value: 'لبن' } });
    await fireEvent.input(screen.getByLabelText(/السعر المرجعي/), {
      target: { value: '150' },
    });
    await fireEvent.change(screen.getByLabelText(/وحدة الشراء/), { target: { value: '2' } });
    await fireEvent.change(screen.getByLabelText(/وحدة الاستهلاك/), { target: { value: '7' } });
    await fireEvent.input(screen.getByLabelText(/معامل التحويل/), { target: { value: '12' } });
    await fireEvent.change(screen.getByLabelText(/تصنيف TVA/), { target: { value: '2' } });

    await fireEvent.click(screen.getByRole('button', { name: 'إنشاء' }));

    await waitFor(() => {
      expect(mockCreateProduct).toHaveBeenCalledTimes(1);
    });

    // Wire contract: numeric codes, not labels (purchase=لتر, consumption=بيضة).
    expect(mockCreateProduct).toHaveBeenCalledWith({
      name: 'لبن',
      base_price: 150,
      purchase_unit: 2,
      consumption_unit: 7,
      conversion_factor: 12,
      tva_classification: 2,
    });
  });

  it('locks the conversion factor to 1 when purchase and consumption units match', async () => {
    render(ProductsPage);
    await waitFor(() => expect(mockListProducts).toHaveBeenCalled());

    await fireEvent.click(screen.getByRole('button', { name: 'إضافة منتج' }));

    await waitFor(() => {
      expect(screen.getByText('منتج جديد')).toBeInTheDocument();
    });

    const factor = screen.getByLabelText(/معامل التحويل/) as HTMLInputElement;
    await fireEvent.input(factor, { target: { value: '9' } });

    await fireEvent.change(screen.getByLabelText(/وحدة الشراء/), { target: { value: '1' } });
    await fireEvent.change(screen.getByLabelText(/وحدة الاستهلاك/), { target: { value: '1' } });

    // Same unit on both axes → factor forced to 1 and field disabled (domain rule).
    expect(factor.disabled).toBe(true);
    expect(factor.value).toBe('1');

    await fireEvent.input(screen.getByLabelText(/الاسم/), { target: { value: 'دقيق' } });
    await fireEvent.input(screen.getByLabelText(/السعر المرجعي/), {
      target: { value: '250' },
    });
    await fireEvent.change(screen.getByLabelText(/تصنيف TVA/), { target: { value: '0' } });

    await fireEvent.click(screen.getByRole('button', { name: 'إنشاء' }));

    await waitFor(() => {
      expect(mockCreateProduct).toHaveBeenCalledTimes(1);
    });
    expect(mockCreateProduct).toHaveBeenCalledWith(
      expect.objectContaining({
        name: 'دقيق',
        purchase_unit: 1,
        consumption_unit: 1,
        conversion_factor: 1,
        tva_classification: 0,
      }),
    );
  });

  it('rejects a create with a missing configuration field', async () => {
    render(ProductsPage);
    await waitFor(() => expect(mockListProducts).toHaveBeenCalled());

    await fireEvent.click(screen.getByRole('button', { name: 'إضافة منتج' }));

    await waitFor(() => {
      expect(screen.getByText('منتج جديد')).toBeInTheDocument();
    });

    await fireEvent.input(screen.getByLabelText(/الاسم/), { target: { value: 'خبز' } });
    await fireEvent.input(screen.getByLabelText(/السعر المرجعي/), {
      target: { value: '10' },
    });
    await fireEvent.change(screen.getByLabelText(/وحدة الشراء/), { target: { value: '10' } });
    // consumption unit + TVA left unselected.

    await fireEvent.click(screen.getByRole('button', { name: 'إنشاء' }));

    await waitFor(() => {
      expect(
        screen.getAllByText('Veuillez sélectionner les unités et la classification TVA')
          .length,
      ).toBeGreaterThan(0);
    });
    expect(mockCreateProduct).not.toHaveBeenCalled();
  });

  it('keeps the update path config-free (no unit/TVA controls in edit modal)', async () => {
    render(ProductsPage);
    await waitFor(() => {
      expect(screen.getByText('دقيق')).toBeInTheDocument();
    });

    await fireEvent.click(screen.getByRole('button', { name: 'تعديل' }));

    await waitFor(() => {
      expect(screen.getByText('تعديل المنتج')).toBeInTheDocument();
    });

    // Update boundary (Phase 6C): the config is immutable through this UI.
    expect(screen.queryByLabelText(/وحدة الشراء/)).toBeNull();
    expect(screen.queryByLabelText(/تصنيف TVA/)).toBeNull();
  });
});