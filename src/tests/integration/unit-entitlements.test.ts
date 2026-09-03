import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/svelte';
import UnitEntitlementsPage from '../../pages/UnitEntitlementsPage.svelte';
import type { UnitContractEntitlement } from '../../lib/types';

// Mock Tauri modules
const mockListUnitContractEntitlements = vi.fn();
const mockGetSettings = vi.fn();

vi.mock('../../lib/tauri', () => ({
  safeInvoke: vi.fn(),
  getAppWindow: () => ({ maximize: vi.fn(), unmaximize: vi.fn(), setResizable: vi.fn(), setMinSize: vi.fn(), setSize: vi.fn(), setMaximizable: vi.fn(), isMaximized: vi.fn().mockResolvedValue(true), center: vi.fn() }),
  createLogicalSize: vi.fn().mockReturnValue({}),
  listenToResize: vi.fn(),
}));

vi.mock('../../lib/contracts', () => ({
  listUnitContractEntitlements: (...args: any[]) => mockListUnitContractEntitlements(...args),
  getSettings: (...args: any[]) => mockGetSettings(...args),
}));

// Mock svelte-spa-router
vi.mock('svelte-spa-router', () => ({
  push: vi.fn(),
  link: {},
}));

function entitlement(overrides: Partial<UnitContractEntitlement> = {}): UnitContractEntitlement {
  return {
    product_id: 'p1',
    product_name: 'دقيق',
    supplier_id: 's1',
    supplier_name: 'المورد المركزي',
    fiscal_year: 2026,
    contracted_quantity: 100,
    fulfilled_quantity: 40,
    released_quantity: 10,
    reserved_quantity: 0,
    effective_remaining: 50,
    entitlement_state: 'Open',
    contract_status: 'ACTIVE',
    agreed_price: 85.5,
    ...overrides,
  };
}

describe('UnitEntitlementsPage (Phase 4 read-only projection)', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockGetSettings.mockResolvedValue({
      node_type: 'UNIT',
      configured: true,
      unit_name: 'مطعم الوحدة 5',
      current_year: 2026,
    });
  });

  it('renders current-year rows with an ACTIVE contract status badge', async () => {
    mockListUnitContractEntitlements.mockResolvedValue([entitlement()]);
    render(UnitEntitlementsPage);

    await waitFor(() => {
      expect(screen.getByText('دقيق')).toBeInTheDocument();
      expect(screen.getByText('المورد المركزي')).toBeInTheDocument();
      expect(screen.getByText('استحقاق السنة الحالية')).toBeInTheDocument();
      expect(screen.getByText('ACTIVE')).toBeInTheDocument();
    });
  });

  it('always shows the static stale-data notice (no persisted timestamp)', async () => {
    mockListUnitContractEntitlements.mockResolvedValue([]);
    render(UnitEntitlementsPage);

    await waitFor(() => {
      expect(screen.getByText('البيانات المعروضة تعكس آخر كتالوج تم استيراده.')).toBeInTheDocument();
    });
  });

  it('shows a prior-year outstanding obligation label without re-deriving quantities', async () => {
    mockListUnitContractEntitlements.mockResolvedValue([
      entitlement({ fiscal_year: 2025, effective_remaining: 20 }),
    ]);
    render(UnitEntitlementsPage);

    await waitFor(() => {
      expect(screen.getByText('التزام سابق قائم')).toBeInTheDocument();
      // Backend-supplied value rendered verbatim (A5) — not recomputed in the page.
      expect(screen.getByText('20.00')).toBeInTheDocument();
    });
  });

  it('labels a fully satisfied prior-year obligation', async () => {
    mockListUnitContractEntitlements.mockResolvedValue([
      entitlement({ fiscal_year: 2024, effective_remaining: 0, released_quantity: 0 }),
    ]);
    render(UnitEntitlementsPage);

    await waitFor(() => {
      expect(screen.getByText('التزام سابق - مستوفى بالكامل')).toBeInTheDocument();
    });
  });

  it('labels a released/closed prior-year obligation', async () => {
    mockListUnitContractEntitlements.mockResolvedValue([
      entitlement({ fiscal_year: 2024, effective_remaining: 0, released_quantity: 60 }),
    ]);
    render(UnitEntitlementsPage);

    await waitFor(() => {
      expect(screen.getByText('التزام سابق - محرر/مغلق')).toBeInTheDocument();
    });
  });

  it('shows CANCELLED rows while keeping them visible in the projection', async () => {
    mockListUnitContractEntitlements.mockResolvedValue([
      entitlement({ contract_status: 'CANCELLED' }),
    ]);
    render(UnitEntitlementsPage);

    await waitFor(() => {
      expect(screen.getByText('CANCELLED')).toBeInTheDocument();
      expect(screen.getByText('دقيق')).toBeInTheDocument();
    });
  });

  it('renders the reserved column only when some row has a reserved quantity', async () => {
    const withoutReserved = entitlement();
    mockListUnitContractEntitlements.mockResolvedValue([withoutReserved]);
    const { unmount } = render(UnitEntitlementsPage);

    await waitFor(() => {
      expect(screen.getByText('دقيق')).toBeInTheDocument();
    });
    // No non-zero reserved value anywhere -> the reserved column header is absent.
    expect(screen.queryByText('المحجوز')).not.toBeInTheDocument();
    unmount();

    const withReserved = entitlement({ reserved_quantity: 25 });
    mockListUnitContractEntitlements.mockResolvedValue([withReserved]);
    render(UnitEntitlementsPage);
    await waitFor(() => {
      expect(screen.getByText('المحجوز')).toBeInTheDocument();
    });
  });

  it('renders the empty state when there are no entitlements', async () => {
    mockListUnitContractEntitlements.mockResolvedValue([]);
    render(UnitEntitlementsPage);

    await waitFor(() => {
      expect(screen.getByText('لا توجد استحقاقات معروضة')).toBeInTheDocument();
    });
  });

  it('propagates a backend error to the user via the danger alert', async () => {
    mockListUnitContractEntitlements.mockRejectedValue(new Error('not allowed'));
    render(UnitEntitlementsPage);

    await waitFor(() => {
      expect(screen.getByText('not allowed')).toBeInTheDocument();
    });
  });
});
