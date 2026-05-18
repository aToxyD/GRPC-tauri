import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, fireEvent, screen, waitFor } from '@testing-library/svelte';
import ConflictCenterPage from '../../pages/ConflictCenterPage.svelte';

// Mock Tauri functions
const mockListSyncConflicts = vi.fn();
const mockResolveSyncConflict = vi.fn();
const mockGetConflictSummary = vi.fn();
const mockGetSettings = vi.fn();

vi.mock('../../lib/tauri', () => ({
    listSyncConflicts: (...args: any[]) => mockListSyncConflicts(...args),
    resolveSyncConflict: (...args: any[]) => mockResolveSyncConflict(...args),
    getConflictSummary: (...args: any[]) => mockGetConflictSummary(...args),
    getSettings: (...args: any[]) => mockGetSettings(...args)
}));
describe('Conflict Center Integration Flow', () => {
    const testConflicts = [
        {
            id: 'conf-1',
            severity: 'CRITICAL',
            conflictType: 'REPLAY_ATTEMPT',
            conflictTypeDisplay: 'محاولة إعادة تشغيل الحزمة',
            packageId: 'pkg-abcd-1234-efgh',
            sourceNodeId: 'node-unit-01',
            createdAt: '2026-05-18T10:00:00Z',
            resolved: false,
            description: 'وجد تكرار لنفس حزمة المزامنة',
            suggestedResolution: {
                description: 'تجاهل الحزمة المستوردة والحفاظ على البيانات الحالية',
                action: 'DISCARD'
            }
        }
    ];

    const testSummary = {
        total: 1,
        unresolved: 1,
        recentConflicts: ['conf-1']
    };

    beforeEach(() => {
        vi.clearAllMocks();
        mockGetSettings.mockResolvedValue({
            node_type: 'WILAYA'
        });
        mockListSyncConflicts.mockResolvedValue(testConflicts);
        mockGetConflictSummary.mockResolvedValue(testSummary);
    });

    it('should render conflicts and summary successfully', async () => {
        render(ConflictCenterPage);

        await waitFor(() => {
            expect(screen.getAllByText('1')[0]).toBeInTheDocument(); // total
            expect(screen.getByText('⏳ معلق')).toBeInTheDocument();
            expect(screen.getByText('pkg-abcd-123...')).toBeInTheDocument();
        });
    });

    it('should open resolution modal and submit resolution successfully', async () => {
        mockResolveSyncConflict.mockResolvedValue(null);

        render(ConflictCenterPage);

        await waitFor(() => {
            expect(screen.getByText('⏳ معلق')).toBeInTheDocument();
        });

        // Open resolve modal
        const resolveBtn = screen.getByText('حل');
        await fireEvent.click(resolveBtn);

        await waitFor(() => {
            expect(screen.getByText('تفاصيل التعارض')).toBeInTheDocument();
        });

        const noteTextarea = screen.getByPlaceholderText(/اذكر سبب الحل وما تم اتخاذه/i);
        const confirmBtn = screen.getByText('✅ تأكيد الحل');

        // Submit button should be disabled initially
        expect(confirmBtn).toBeDisabled();

        // Type note
        await fireEvent.input(noteTextarea, { target: { value: 'تم التنسيق مع العقدة وتجاوز التكرار' } });
        expect(confirmBtn).not.toBeDisabled();

        // Submit
        await fireEvent.click(confirmBtn);

        await waitFor(() => {
            expect(mockResolveSyncConflict).toHaveBeenCalledWith('conf-1', 'تم التنسيق مع العقدة وتجاوز التكرار');
            expect(screen.queryByText('تفاصيل التعارض')).not.toBeInTheDocument();
        });
    });
});
