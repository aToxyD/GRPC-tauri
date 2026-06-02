import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { get } from 'svelte/store';
import { createRuntimeScope } from '../../lib/runtimeCleanup';
import type { RuntimeScope } from '../../lib/runtimeCleanup';
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
    let sessionScope: RuntimeScope;

    beforeEach(() => {
        vi.clearAllMocks();
        vi.spyOn(console, 'error').mockImplementation(() => {});
        currentUser.set(null);
        sessionState.set({
            isActive: false,
            warningShown: false
        });
        sessionScope = createRuntimeScope();
    });

    afterEach(() => {
        sessionScope.dispose();
        cleanupSessionManagement();
    });

    describe('bootstrapSession', () => {
        it('should fetch user from backend and set it', async () => {
            const user = { username: 'testuser', role: 'admin' };
            mockInvoke.mockResolvedValueOnce(user);
            mockInvoke.mockResolvedValueOnce({ is_active: true, is_expired: false, should_warn: false, remaining_minutes: 60 });
            
            const result = await bootstrapSession(sessionScope);
            
            expect(mockInvoke).toHaveBeenCalledWith('get_current_user', undefined);
            expect(result).toEqual(user);
            expect(get(currentUser)).toEqual(user);
        });

        it('should handle failure and return null', async () => {
            mockInvoke.mockRejectedValueOnce(new Error('Backend error'));
            
            const result = await bootstrapSession(sessionScope);
            
            expect(result).toBeNull();
            expect(get(currentUser)).toBeNull();
        });
    });

    describe('logout flow', () => {
        it('should call backend logout and redirect', async () => {
            mockInvoke.mockResolvedValueOnce(null);
            
            await logout();
            
            expect(mockInvoke).toHaveBeenCalledWith('logout', undefined);
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
            
            mockInvoke.mockResolvedValueOnce({ username: 'test' });
            mockInvoke.mockResolvedValueOnce({ is_active: false, is_expired: true, should_warn: false });
            mockInvoke.mockResolvedValueOnce(null);
            
            await bootstrapSession(sessionScope);
            
            await new Promise((r) => setTimeout(r, 10));
            
            expect(mockShowError).toHaveBeenCalledWith('الرجاء تسجيل الدخول مرة أخرى', 'انتهت الجلسة');
            expect(mockPush).toHaveBeenCalledWith('/login');
        });

        it('should warn when session is about to expire', async () => {
            mockInvoke.mockResolvedValueOnce({ username: 'test' });
            mockInvoke.mockResolvedValueOnce({ 
                is_active: true, 
                is_expired: false, 
                should_warn: true, 
                remaining_minutes: 4 
            });
            
            await bootstrapSession(sessionScope);
            await new Promise((r) => setTimeout(r, 10));
            
            expect(mockShowWarning).toHaveBeenCalled();
            expect(get(sessionState).warningShown).toBe(true);
        });
    });
});
