import { writable, type Writable } from 'svelte/store';
import { push } from 'svelte-spa-router';
import { showError, showWarning, showInfo } from './notifications';
import type { SessionStatus, User } from './types';
import { getCurrentUser, touchSession as tauriTouchSession, logout as tauriLogout, checkSession as tauriCheckSession } from './contracts';
import { createRuntimeScope, type RuntimeScope } from './runtimeCleanup';

// @category SessionState
export const currentUser = writable<User | null>(null);

// @category SessionState — active session flag (module-level for non-reactive access)
let isActive = false;

// @category UiState — warning display state (module-level, not in store)
let warningShown = false;

// Lifecycle container for all session timers and listeners
let activeScope: RuntimeScope | null = null;

// @category SessionState — reactive session state for subscriptions
export const sessionActive: Writable<boolean> = writable(false);

// @category SessionState — backward-compatible session state
interface SessionState {
    isActive: boolean;
    warningShown: boolean;
}

// @category SessionState — combined session state
export const sessionState: Writable<SessionState> = writable({
    isActive: false,
    warningShown: false
});

export function setCurrentUser(user: User | null) {
    currentUser.set(user);
}

export async function bootstrapSession(scope?: RuntimeScope): Promise<User | null> {
    try {
        const user = await getCurrentUser();
        setCurrentUser(user);
        if (user) {
            startSessionMonitoring(scope ?? createRuntimeScope());
        }
        return user;
    } catch (error) {
        console.error('Failed to bootstrap session', error);
        return null;
    }
}

const SESSION_CHECK_INTERVAL = 60000;
const ACTIVITY_DEBOUNCE = 5000;

const TRACKED_EVENTS = ['mousedown', 'keydown', 'touchstart', 'scroll'] as const;

export async function checkSession(): Promise<SessionStatus | null> {
    try {
        const status = await tauriCheckSession();
        isActive = status.is_active;
        sessionActive.set(status.is_active);
        sessionState.update(s => ({ ...s, isActive: status.is_active }));
        return status;
    } catch (error) {
        return null;
    }
}

function handleSessionStatus(status: SessionStatus): void {
    if (!status.is_active) {
        if (status.is_expired) {
            showError('الرجاء تسجيل الدخول مرة أخرى', 'انتهت الجلسة');
            logout();
        }
        return;
    }

    if (status.should_warn && !warningShown) {
        showWarning(
            `ستنتهي جلستك خلال ${status.remaining_minutes} دقيقة. قم بأي نشاط للتمديد.`,
            'تحذير الجلسة'
        );
        warningShown = true;
        sessionState.update(s => ({ ...s, warningShown: true }));
    }

    if (!status.should_warn && warningShown) {
        warningShown = false;
        sessionState.update(s => ({ ...s, warningShown: false }));
    }
}

export function startSessionMonitoring(scope: RuntimeScope): void {
    stopSessionMonitoring();
    activeScope = scope;

    checkSession().then(status => {
        if (!scope.isAlive()) return;
        if (status) handleSessionStatus(status);
    });

    scope.setInterval(() => {
        checkSession().then(status => {
            if (!scope.isAlive()) return;
            if (status) handleSessionStatus(status);
        });
    }, SESSION_CHECK_INTERVAL);

    setupActivityTracking(scope);
}

export function stopSessionMonitoring(): void {
    if (activeScope) {
        activeScope.dispose();
        activeScope = null;
    }
    isActive = false;
    warningShown = false;
    sessionActive.set(false);
    sessionState.set({ isActive: false, warningShown: false });
}

function setupActivityTracking(scope: RuntimeScope): void {
    let debounceTimer: number | null = null;

    const handler = () => {
        if (debounceTimer !== null) {
            clearTimeout(debounceTimer);
        }
        debounceTimer = scope.setTimeout(() => {
            touchSession();
            warningShown = false;
            sessionState.update(s => ({ ...s, warningShown: false }));
            debounceTimer = null;
        }, ACTIVITY_DEBOUNCE);
    };

    TRACKED_EVENTS.forEach(event => {
        scope.addListener(document, event, handler as EventListener, { passive: true });
    });
}

async function touchSession(): Promise<void> {
    try {
        await tauriTouchSession();
    } catch (error) {
        // Failed to touch session
    }
}

export async function logout(): Promise<void> {
    try {
        await tauriLogout();
        setCurrentUser(null);
        showInfo('تم تسجيل خروجك بنجاح', 'تم تسجيل الخروج');
        push('/login');
    } catch (error) {
        push('/login');
    } finally {
        stopSessionMonitoring();
    }
}

export function getSessionInfo(): { isActive: boolean } {
    return { isActive };
}

export function initSessionManagement(scope?: RuntimeScope): void {
    startSessionMonitoring(scope ?? createRuntimeScope());
}

export function cleanupSessionManagement(): void {
    stopSessionMonitoring();
}
