import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, fireEvent, screen, waitFor } from '@testing-library/svelte';
import FiscalManagementPage from '../../pages/FiscalManagementPage.svelte';

// Mock Tauri functions
const mockCloseFiscalYear = vi.fn();
const mockGetFiscalYearStatus = vi.fn();
const mockListProducts = vi.fn();
const mockGetSettings = vi.fn();
const mockGetFiscalTransitionHistory = vi.fn();
const mockListFiscalPackageRegistry = vi.fn();
const mockPreviewFiscalClosurePackage = vi.fn();
const mockApplyFiscalClosurePackage = vi.fn();

const mockShowAsk = vi.fn();

vi.mock('../../lib/tauri', () => ({
    closeFiscalYear: (...args: any[]) => mockCloseFiscalYear(...args),
    getFiscalYearStatus: (...args: any[]) => mockGetFiscalYearStatus(...args),
    listProducts: (...args: any[]) => mockListProducts(...args),
    getSettings: (...args: any[]) => mockGetSettings(...args),
    getFiscalTransitionHistory: (...args: any[]) => mockGetFiscalTransitionHistory(...args),
    listFiscalPackageRegistry: (...args: any[]) => mockListFiscalPackageRegistry(...args),
    previewFiscalClosurePackage: (...args: any[]) => mockPreviewFiscalClosurePackage(...args),
    applyFiscalClosurePackage: (...args: any[]) => mockApplyFiscalClosurePackage(...args),
    showAsk: (...args: any[]) => mockShowAsk(...args),
    getAppWindow: () => ({ maximize: vi.fn(), setResizable: vi.fn(), setMinSize: vi.fn(), setMaximizable: vi.fn(), isMaximized: vi.fn().mockResolvedValue(true) }),
    listenToResize: vi.fn().mockResolvedValue(() => {}),
}));

describe('Fiscal Management Integration Flow', () => {
    beforeEach(() => {
        vi.clearAllMocks();
        mockShowAsk.mockResolvedValue(true);

        mockGetSettings.mockResolvedValue({
            node_type: 'WILAYA',
            current_year: 2026
        });
        mockGetFiscalYearStatus.mockResolvedValue({
            year: 2026,
            status: 'open'
        });
        mockListProducts.mockResolvedValue([]);
        mockGetFiscalTransitionHistory.mockResolvedValue([]);
        mockListFiscalPackageRegistry.mockResolvedValue([]);
    });

    it('should prevent execution of fiscal close without correct confirmation text', async () => {
        render(FiscalManagementPage);

        await waitFor(() => {
            expect(screen.getByText(/سلطة إغلاق السنة المالية/i)).toBeInTheDocument();
        });

        const input = screen.getByLabelText(/اكتب رقم السنة الحالية/i);
        const btn = screen.getByRole('button', { name: /إصدار قرار إغلاق السنة/i });

        // Input incorrect year
        await fireEvent.input(input, { target: { value: '2025' } });
        await fireEvent.click(btn);

        await waitFor(() => {
            expect(screen.getByText(/رقم السنة غير صحيح. تم رفض العملية/i)).toBeInTheDocument();
            expect(mockCloseFiscalYear).not.toHaveBeenCalled();
        });
    });

    it('should prompt confirm dialog and close fiscal year successfully when input matches', async () => {
        mockCloseFiscalYear.mockResolvedValue({
            closed_year: 2026,
            opened_year: 2027
        });

        render(FiscalManagementPage);

        await waitFor(() => {
            expect(screen.getByText(/سلطة إغلاق السنة المالية/i)).toBeInTheDocument();
        });

        const input = screen.getByLabelText(/اكتب رقم السنة الحالية/i);
        const btn = screen.getByRole('button', { name: /إصدار قرار إغلاق السنة/i });

        // Input correct year
        await fireEvent.input(input, { target: { value: '2026' } });
        await fireEvent.click(btn);

        expect(mockShowAsk).toHaveBeenCalledWith(
            expect.stringContaining('سيتم إغلاق السنة المالية 2026'),
            expect.any(Object)
        );

        await waitFor(() => {
            expect(mockCloseFiscalYear).toHaveBeenCalledWith({ year: 2026, next_year: 2027 });
            expect(screen.getByText(/تم إغلاق 2026 وفتح 2027 بنجاح/i)).toBeInTheDocument();
        });
    });
});
