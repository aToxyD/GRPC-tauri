import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { get } from 'svelte/store';
import { 
    bootstrapSession, 
    logout, 
    checkSession, 
    currentUser, 
    sessionState,
    cleanupSessionManagement
} from '../../lib/session';

// Mock Tauri invoke
const mockInvoke = vi.fn();
vi.mock('@tauri-apps/api/core', () => ({
    invoke: (...args: any[]) => mockInvoke(...args)
}));

// Mock Svelte SPA Router
const mockPush = vi.fn();
vi.mock('svelte-spa-router', () => ({
    push: (...args: any[]) => mockPush(...args)
}));

// Mock Notifications
const mockShowError = vi.fn();
const mockShowWarning = vi.fn();
const mockShowInfo = vi.fn();
vi.mock('../../lib/notifications', () => ({
    showError: (...args: any[]) => mockShowError(...args),
    showWarning: (...args: any[]) => mockShowWarning(...args),
    showInfo: (...args: any[]) => mockShowInfo(...args)
}));

describe('Session Store', () => {
    beforeEach(() => {
        vi.clearAllMocks();
        vi.useFakeTimers();
        currentUser.set(null);
        sessionState.set({
            isActive: false,
            lastCheck: null,
            warningShown: false,
            checkInterval: null
        });
    });

    afterEach(() => {
        cleanupSessionManagement();
        vi.useRealTimers();
    });

    describe('bootstrapSession', () => {
        it('should fetch user from backend and set it', async () => {
            const user = { username: 'testuser', role: 'admin' };
            mockInvoke.mockResolvedValueOnce(user);
            mockInvoke.mockResolvedValueOnce({ is_active: true, is_expired: false, should_warn: false, remaining_minutes: 60 }); // Initial check_session
            
            const result = await bootstrapSession();
            
            expect(mockInvoke).toHaveBeenCalledWith('get_current_user');
            expect(result).toEqual(user);
            expect(get(currentUser)).toEqual(user);
        });

        it('should handle failure and return null', async () => {
            mockInvoke.mockRejectedValueOnce(new Error('Backend error'));
            
            const result = await bootstrapSession();
            
            expect(result).toBeNull();
            expect(get(currentUser)).toBeNull();
        });
    });

    describe('logout flow', () => {
        it('should call backend logout and redirect', async () => {
            mockInvoke.mockResolvedValueOnce(null); // backend logout response
            
            await logout();
            
            expect(mockInvoke).toHaveBeenCalledWith('logout');
            expect(get(currentUser)).toBeNull();
            expect(mockShowInfo).toHaveBeenCalled();
            expect(mockPush).toHaveBeenCalledWith('/login');
        });

        it('should force redirect even if backend fails', async () => {
            mockInvoke.mockRejectedValueOnce(new Error('Network error'));
            
            await logout();
            
            expect(get(currentUser)).toBeNull();
            expect(mockPush).toHaveBeenCalledWith('/login');
        });
    });

    describe('session monitoring', () => {
        it('should auto-logout on expiration', async () => {
            mockInvoke.mockResolvedValueOnce({ 
                is_active: false, 
                is_expired: true, 
                should_warn: false, 
                remaining_minutes: 0 
            });
            
            await checkSession();
            
            // Check session manually triggers status handler if integrated properly.
            // Wait, checkSession returns the status, but interval handles it. 
            // Let's test the status handling by starting monitoring.
            
            // Re-bootstrap to start monitoring
            mockInvoke.mockResolvedValueOnce({ username: 'test' }); // get_user
            mockInvoke.mockResolvedValueOnce({ is_active: false, is_expired: true, should_warn: false }); // check_session
            mockInvoke.mockResolvedValueOnce(null); // logout
            
            await bootstrapSession();
            
            // allow promises and microtasks to resolve
            await vi.advanceTimersByTimeAsync(10);
            
            expect(mockShowError).toHaveBeenCalledWith('الرجاء تسجيل الدخول مرة أخرى', 'انتهت الجلسة');
            expect(mockPush).toHaveBeenCalledWith('/login');
        });

        it('should warn when session is about to expire', async () => {
            mockInvoke.mockResolvedValueOnce({ username: 'test' }); // get_user
            mockInvoke.mockResolvedValueOnce({ 
                is_active: true, 
                is_expired: false, 
                should_warn: true, 
                remaining_minutes: 4 
            }); // check_session
            
            await bootstrapSession();
            await vi.advanceTimersByTimeAsync(10);
            
            expect(mockShowWarning).toHaveBeenCalled();
            expect(get(sessionState).warningShown).toBe(true);
        });
    });
});
