import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, fireEvent, screen, waitFor } from '@testing-library/svelte';
import ContractsPage from '../../pages/ContractsPage.svelte';
import type { Contract, Product } from '../../lib/types';

// Mock Tauri modules
const mockSaveFile = vi.fn();
const mockShowAsk = vi.fn();

vi.mock('../../lib/tauri', () => ({
  safeInvoke: vi.fn(),
  saveFile: (...args: any[]) => mockSaveFile(...args),
  showAsk: (...args: any[]) => mockShowAsk(...args),
  getAppWindow: () => ({ maximize: vi.fn(), unmaximize: vi.fn(), setResizable: vi.fn(), setMinSize: vi.fn(), setSize: vi.fn(), setMaximizable: vi.fn(), isMaximized: vi.fn().mockResolvedValue(true), center: vi.fn() }),
  createLogicalSize: vi.fn().mockReturnValue({}),
  listenToResize: vi.fn(),
}));

// Mock the contracts IPC surface used by ContractsPage
const mockListContracts = vi.fn();
const mockListUnitSuppliers = vi.fn();
const mockGetContractProducts = vi.fn();
const mockListContractAllocations = vi.fn();
const mockListAllocationExceptions = vi.fn();
const mockCreateContract = vi.fn();
const mockAddContractProduct = vi.fn();
const mockSetContractProductAgreedPriceHt = vi.fn();
const mockAcceptContract = vi.fn();
const mockActivateContract = vi.fn();
const mockEndContract = vi.fn();
const mockCancelContract = vi.fn();
const mockReleaseContractAllocation = vi.fn();
const mockRevokeContractAllocationRelease = vi.fn();
const mockExportContractsExcel = vi.fn();
const mockExportContractAllocationsExcel = vi.fn();
const mockExportContractCatalogPackage = vi.fn();
const mockListUnits = vi.fn();
const mockListSuppliers = vi.fn();
const mockListProducts = vi.fn();
const mockGetSettings = vi.fn();

vi.mock('../../lib/contracts', () => ({
  listContracts: (...args: any[]) => mockListContracts(...args),
  listUnitSuppliers: (...args: any[]) => mockListUnitSuppliers(...args),
  getContractProducts: (...args: any[]) => mockGetContractProducts(...args),
  listContractAllocations: (...args: any[]) => mockListContractAllocations(...args),
  listAllocationExceptions: (...args: any[]) => mockListAllocationExceptions(...args),
  createContract: (...args: any[]) => mockCreateContract(...args),
  addContractProduct: (...args: any[]) => mockAddContractProduct(...args),
  setContractProductAgreedPriceHt: (...args: any[]) => mockSetContractProductAgreedPriceHt(...args),
  acceptContract: (...args: any[]) => mockAcceptContract(...args),
  activateContract: (...args: any[]) => mockActivateContract(...args),
  endContract: (...args: any[]) => mockEndContract(...args),
  cancelContract: (...args: any[]) => mockCancelContract(...args),
  releaseContractAllocation: (...args: any[]) => mockReleaseContractAllocation(...args),
  revokeContractAllocationRelease: (...args: any[]) => mockRevokeContractAllocationRelease(...args),
  exportContractsExcel: (...args: any[]) => mockExportContractsExcel(...args),
  exportContractAllocationsExcel: (...args: any[]) => mockExportContractAllocationsExcel(...args),
  exportContractCatalogPackage: (...args: any[]) =>
    mockExportContractCatalogPackage(...args),
  listUnits: (...args: any[]) => mockListUnits(...args),
  listSuppliers: (...args: any[]) => mockListSuppliers(...args),
  listProducts: (...args: any[]) => mockListProducts(...args),
  getSettings: (...args: any[]) => mockGetSettings(...args),
}));

// Mock svelte-spa-router (used by Layout/Sidebar)
vi.mock('svelte-spa-router', () => ({
  push: vi.fn(),
  link: {},
}));

const contract: Contract = {
  id: 'c1',
  unit_id: 'u1',
  supplier_id: 's1',
  fiscal_year: 2026,
  status: 'Proposed',
  contract_reference: 'C-2026-001',
  proposed_at: null,
  accepted_at: null,
  activated_at: null,
  ended_at: null,
  cancelled_at: null,
  notes: null,
  created_at: null,
};

const product: Product = {
  id: 'p1',
  name: 'دقيق',
  base_price: 120,
  year: 2026,
  created_at: '2026-01-01T00:00:00Z',
};

/**
 * Drive the real ContractsPage add-product UI path:
 *   select Proposed contract -> "إضافة منتج" -> fill required fields ->
 *   exercise the optional agreed-price input via `setupAgreed` -> submit "إضافة".
 *
 * Returns the AddContractProductRequest payload captured by the mocked IPC call.
 */
async function submitAddProduct(
  setupAgreed: (input: HTMLInputElement) => Promise<void> | void,
): Promise<any> {
  // Wait for the contracts projection (from mocked listContracts) to render.
  await waitFor(() => {
    expect(screen.getByText('C-2026-001')).toBeInTheDocument();
  });
  fireEvent.click(screen.getByText('C-2026-001'));
  await waitFor(() => {
    expect(screen.getByText('إضافة منتج')).toBeInTheDocument();
  });
  fireEvent.click(screen.getByText('إضافة منتج'));

  const productSelect = (await screen.findByLabelText(/المنتج/)) as HTMLSelectElement;
  await fireEvent.change(productSelect, { target: { value: product.id } });

  const proposedPrice = screen.getByLabelText(/السعر المقترح/) as HTMLInputElement;
  await fireEvent.input(proposedPrice, { target: { value: '100' } });

  const agreedPrice = screen.getByLabelText(/سعر الاتفاق/) as HTMLInputElement;
  await setupAgreed(agreedPrice);

  const quantity = screen.getByLabelText(/الكمية المتفق عليها/) as HTMLInputElement;
  await fireEvent.input(quantity, { target: { value: '10' } });

  fireEvent.click(screen.getByText('إضافة'));

  await waitFor(() => {
    expect(mockAddContractProduct).toHaveBeenCalled();
  });

  const calls = mockAddContractProduct.mock.calls;
  return calls[calls.length - 1][0];
}

describe('ContractsPage add-product agreed_price_ht normalization', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockAddContractProduct.mockResolvedValue(['cp1', 'a1']);
    mockGetContractProducts.mockResolvedValue([]);
    mockListContractAllocations.mockResolvedValue([]);
    mockListAllocationExceptions.mockResolvedValue([]);
    mockListUnitSuppliers.mockResolvedValue([]);
    mockListSuppliers.mockResolvedValue([]);
    mockListUnits.mockResolvedValue([]);
    mockListProducts.mockResolvedValue([product]);
    mockListContracts.mockResolvedValue([contract]);
    mockGetSettings.mockResolvedValue({ current_year: 2026 });
  });

  it('sends agreed_price_ht null when the optional field is left untouched (empty string)', async () => {
    render(ContractsPage);
    const request = await submitAddProduct(async () => {
      // no interaction -> newAgreedPrice stays the initial ''
    });
    expect(request.agreed_price_ht).toBeNull();
  });

  it('sends the parsed number for a numeric agreed HT price', async () => {
    render(ContractsPage);
    const request = await submitAddProduct(async (input) => {
      await fireEvent.input(input, { target: { value: '123.45' } });
    });
    expect(request.agreed_price_ht).toBe(123.45);
  });

  it('sends a decimal agreed HT price correctly', async () => {
    render(ContractsPage);
    const request = await submitAddProduct(async (input) => {
      await fireEvent.input(input, { target: { value: '0.5' } });
    });
    expect(request.agreed_price_ht).toBe(0.5);
  });

  it('sends 0 as a distinct agreed HT value (not null)', async () => {
    render(ContractsPage);
    const request = await submitAddProduct(async (input) => {
      await fireEvent.input(input, { target: { value: '0' } });
    });
    expect(request.agreed_price_ht).toBe(0);
  });

  it('sends agreed_price_ht null after a numeric input is typed then cleared (number input coerces to null)', async () => {
    render(ContractsPage);
    const request = await submitAddProduct(async (input) => {
      await fireEvent.input(input, { target: { value: '50' } });
      // Clearing a type="number" input coerces the bound signal to null (Svelte
      // to_number returns null for ''). The normalization must not call .trim()
      // on null (previously: "t(wt).trim is not a function").
      await fireEvent.input(input, { target: { value: '' } });
    });
    expect(request.agreed_price_ht).toBeNull();
  });
});
