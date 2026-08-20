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
const mockGetIdentityStatus = vi.fn();
const mockGetSecurityStatus = vi.fn();
const mockBeginWilayaProvision = vi.fn();
const mockFinalizeWilayaProvision = vi.fn();
const mockIssueFirstAdminKey = vi.fn();
const mockBeginUnitProvision = vi.fn();
const mockFinalizeUnitProvision = vi.fn();

vi.mock('../../lib/tauri', () => ({
    safeInvoke: vi.fn(),
    openFile: vi.fn(),
    getAppWindow: () => ({ maximize: vi.fn(), unmaximize: vi.fn(), setResizable: vi.fn(), setMinSize: vi.fn(), setSize: vi.fn(), setMaximizable: vi.fn(), isMaximized: vi.fn().mockResolvedValue(true), center: vi.fn() }),
    createLogicalSize: vi.fn().mockReturnValue({}),
}));

vi.mock('../../lib/contracts', () => ({
    login: (...args: any[]) => mockLogin(...args),
    isConfigured: (...args: any[]) => mockIsConfigured(...args),
    getSettings: (...args: any[]) => mockGetSettings(...args),
    importUnitNodePackage: (...args: any[]) => mockImportUnitNodePackage(...args),
    importIdentityAccessPackage: vi.fn(),
    getIdentityStatus: (...args: any[]) => mockGetIdentityStatus(...args),
    getSecurityStatus: (...args: any[]) => mockGetSecurityStatus(...args),
    beginWilayaProvision: (...args: any[]) => mockBeginWilayaProvision(...args),
    finalizeWilayaProvision: (...args: any[]) => mockFinalizeWilayaProvision(...args),
    issueFirstAdminKey: (...args: any[]) => mockIssueFirstAdminKey(...args),
    beginUnitProvision: (...args: any[]) => mockBeginUnitProvision(...args),
    finalizeUnitProvision: (...args: any[]) => mockFinalizeUnitProvision(...args),
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
        mockGetIdentityStatus.mockResolvedValue('UNINITIALIZED');
        mockGetSecurityStatus.mockResolvedValue({ requires_action: false });
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
            requires_configuration: false,
            identity_challenge_required: false
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
            requires_configuration: false,
            identity_challenge_required: false
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

    it('should hide the password tab and default to admin-key when the node has an ACTIVE ADMIN identity', async () => {
        mockGetIdentityStatus.mockResolvedValue('READY');

        render(LoginPage);

        await waitFor(() => {
            expect(screen.queryByText('كلمة المرور')).not.toBeInTheDocument();
            expect(screen.getByPlaceholderText(/أدخل كلمة مرور المفتاح/i)).toBeInTheDocument();
        });
    });

    it('should route to the admin-key tab when the backend reports identity_challenge_required', async () => {
        mockLogin.mockResolvedValue({
            success: false,
            user: null,
            message: 'عليك تسجيل الدخول باستخدام المفتاح الإداري',
            requires_configuration: false,
            identity_challenge_required: true
        });

        render(LoginPage);

        const usernameInput = screen.getByPlaceholderText(/اسم المستخدم/i);
        const passwordInput = screen.getByPlaceholderText(/كلمة المرور/i);
        const submitBtn = screen.getByRole('button', { name: /تسجيل الدخول/i });

        await fireEvent.input(usernameInput, { target: { value: 'civil_admin' } });
        await fireEvent.input(passwordInput, { target: { value: 'supersecret' } });
        await fireEvent.click(submitBtn);

        await waitFor(() => {
            expect(screen.getByText(/عليك تسجيل الدخول باستخدام المفتاح الإداري/i)).toBeInTheDocument();
            expect(screen.getByPlaceholderText(/أدخل كلمة مرور المفتاح/i)).toBeInTheDocument();
        });
    });
});
