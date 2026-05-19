import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, fireEvent, screen, waitFor } from '@testing-library/svelte';
import LoginPage from '../../pages/LoginPage.svelte';
import { currentUser } from '../../lib/session';
import { get } from 'svelte/store';

// Mock Tauri modules
const mockLogin = vi.fn();
const mockIsConfigured = vi.fn();
const mockGetSettings = vi.fn();
const mockImportUnitNodePackage = vi.fn();

vi.mock('../../lib/tauri', () => ({
    login: (...args: any[]) => mockLogin(...args),
    isConfigured: (...args: any[]) => mockIsConfigured(...args),
    getSettings: (...args: any[]) => mockGetSettings(...args),
    importUnitNodePackage: (...args: any[]) => mockImportUnitNodePackage(...args),
    getAppWindow: () => ({ maximize: vi.fn(), unmaximize: vi.fn(), setResizable: vi.fn(), setMinSize: vi.fn(), setSize: vi.fn(), setMaximizable: vi.fn(), isMaximized: vi.fn().mockResolvedValue(true), center: vi.fn() }),
    createLogicalSize: vi.fn().mockReturnValue({}),
}));

// Mock tauri plugin dialog
vi.mock('@tauri-apps/plugin-dialog', () => ({
    open: vi.fn()
}));

// Mock tauri api window
const mockMaximize = vi.fn();
vi.mock('@tauri-apps/api/window', () => ({
    getCurrentWindow: () => ({
        maximize: mockMaximize
    })
}));

// Mock svelte-spa-router
const mockPush = vi.fn();
vi.mock('svelte-spa-router', () => ({
    push: (...args: any[]) => mockPush(...args)
}));

describe('Login Page Integration Flow', () => {
    beforeEach(() => {
        vi.clearAllMocks();
        currentUser.set(null);
        mockIsConfigured.mockResolvedValue(true);
    });

    it('should display error if fields are submitted empty', async () => {
        render(LoginPage);

        const submitBtn = screen.getByRole('button', { name: /تسجيل الدخول/i });
        await fireEvent.click(submitBtn);

        await waitFor(() => {
            expect(screen.getByText(/الرجاء إدخال اسم المستخدم وكلمة المرور/i)).toBeInTheDocument();
        });
    });

    it('should login successfully and redirect based on settings', async () => {
        const user = { username: 'civil_admin', role: 'Admin' };
        mockLogin.mockResolvedValue({
            success: true,
            user,
            message: 'تم تسجيل الدخول',
            requires_configuration: false
        });
        mockGetSettings.mockResolvedValue({
            node_type: 'WILAYA',
            configured: true
        });

        render(LoginPage);

        const usernameInput = screen.getByPlaceholderText(/اسم المستخدم/i);
        const passwordInput = screen.getByPlaceholderText(/كلمة المرور/i);
        const submitBtn = screen.getByRole('button', { name: /تسجيل الدخول/i });

        await fireEvent.input(usernameInput, { target: { value: 'civil_admin' } });
        await fireEvent.input(passwordInput, { target: { value: 'supersecret' } });
        await fireEvent.click(submitBtn);

        await waitFor(() => {
            expect(mockLogin).toHaveBeenCalledWith({ username: 'civil_admin', password: 'supersecret' });
            expect(get(currentUser)).toEqual(user);
            expect(mockPush).toHaveBeenCalledWith('/wilaya');
        });
    });

    it('should handle invalid credentials and lockout propagation from backend', async () => {
        mockLogin.mockResolvedValue({
            success: false,
            user: null,
            message: 'تجاوز الحد الأقصى للمحاولات المقبولة. تم الحظر مؤقتاً',
            requires_configuration: false
        });

        render(LoginPage);

        const usernameInput = screen.getByPlaceholderText(/اسم المستخدم/i);
        const passwordInput = screen.getByPlaceholderText(/كلمة المرور/i);
        const submitBtn = screen.getByRole('button', { name: /تسجيل الدخول/i });

        await fireEvent.input(usernameInput, { target: { value: 'invalid_user' } });
        await fireEvent.input(passwordInput, { target: { value: 'wrongpass' } });
        await fireEvent.click(submitBtn);

        await waitFor(() => {
            expect(screen.getByText(/تجاوز الحد الأقصى للمحاولات المقبولة/i)).toBeInTheDocument();
            expect(get(currentUser)).toBeNull();
        });
    });
});
