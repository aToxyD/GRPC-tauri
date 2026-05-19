/**
 * Session Management for GRPC Frontend
 * 
 * Provides:
 * - Automatic session checking every 60 seconds
 * - Warning before session expiry (5 minutes)
 * - Auto-logout on session expiration
 * - Activity tracking to extend session
 */

import { writable, type Writable } from 'svelte/store';
import { push } from 'svelte-spa-router';
import { showError, showWarning, showInfo } from './notifications';
import type { SessionStatus, User } from './types';
import { getCurrentUser, touchSession as tauriTouchSession, logout as tauriLogout, checkSession as tauriCheckSession } from './tauri';

// Auth State
export const currentUser = writable<User | null>(null);

export function setCurrentUser(user: User | null) {
    currentUser.set(user);
}

export async function bootstrapSession(): Promise<User | null> {
    try {
        const user = await getCurrentUser();
        setCurrentUser(user);
        if (user) {
            startSessionMonitoring();
        }
        return user;
    } catch (error) {
        console.error('Failed to bootstrap session', error);
        return null;
    }
}


// Session check interval in milliseconds (60 seconds)
const SESSION_CHECK_INTERVAL = 60000;

// Warning threshold in minutes (5 minutes before expiry)
const SESSION_WARNING_MINUTES = 5;

// Activity debounce time in milliseconds (5 seconds)
const ACTIVITY_DEBOUNCE = 5000;

interface SessionState {
    isActive: boolean;
    lastCheck: Date | null;
    warningShown: boolean;
    checkInterval: number | null;
}

export const sessionState: Writable<SessionState> = writable({
    isActive: false,
    lastCheck: null,
    warningShown: false,
    checkInterval: null
});

let activityTimeout: number | null = null;
let activityHandler: (() => void) | null = null;
const TRACKED_EVENTS = ['mousedown', 'keydown', 'touchstart', 'scroll'] as const;

/**
 * Check current session status
 */
export async function checkSession(): Promise<SessionStatus | null> {
    try {
        const status = await tauriCheckSession();
        sessionState.update(s => ({
            ...s,
            lastCheck: new Date(),
            isActive: status.is_active
        }));
        return status;
    } catch (error) {
        return null;
    }
}

/**
 * Handle session status and warnings
 */
function handleSessionStatus(status: SessionStatus): void {
    if (!status.is_active) {
        // Session expired or no session
        if (status.is_expired) {
            showError('الرجاء تسجيل الدخول مرة أخرى', 'انتهت الجلسة');
            logout();
        }
        return;
    }

    // Show warning if session is about to expire
    let warningAlreadyShown = false;
    sessionState.subscribe(s => warningAlreadyShown = s.warningShown)();

    if (status.should_warn && !warningAlreadyShown) {
        showWarning(
            `ستنتهي جلستك خلال ${status.remaining_minutes} دقيقة. قم بأي نشاط للتمديد.`,
            'تحذير الجلسة'
        );
        sessionState.update(s => ({ ...s, warningShown: true }));
    }

    // Reset warning if session is no longer in warning state
    if (!status.should_warn && warningAlreadyShown) {
        sessionState.update(s => ({ ...s, warningShown: false }));
    }
}

/**
 * Start session monitoring
 */
export function startSessionMonitoring(): void {
    // Stop any existing monitoring
    stopSessionMonitoring();

    // Initial check
    checkSession().then(status => {
        if (status) {
            handleSessionStatus(status);
        }
    });

    // Set up periodic checks
    const interval = window.setInterval(() => {
        checkSession().then(status => {
            if (status) {
                handleSessionStatus(status);
            }
        });
    }, SESSION_CHECK_INTERVAL);

    sessionState.update(s => ({ ...s, checkInterval: interval }));

    // Track user activity
    setupActivityTracking();
}

/**
 * Stop session monitoring
 */
export function stopSessionMonitoring(): void {
    sessionState.update(s => {
        if (s.checkInterval !== null) {
            clearInterval(s.checkInterval);
        }
        return { ...s, checkInterval: null };
    });
    
    if (activityTimeout !== null) {
        clearTimeout(activityTimeout);
        activityTimeout = null;
    }

    removeActivityTracking();
}

/**
 * Remove activity tracking event listeners
 */
function removeActivityTracking(): void {
    if (activityHandler) {
        TRACKED_EVENTS.forEach(event => {
            document.removeEventListener(event, activityHandler!);
        });
        activityHandler = null;
    }
}

/**
 * Setup activity tracking to extend session
 */
function setupActivityTracking(): void {
    // Remove existing listeners first to avoid duplication
    removeActivityTracking();

    activityHandler = () => {
        // Debounce activity to avoid excessive API calls
        if (activityTimeout !== null) {
            clearTimeout(activityTimeout);
        }
        
        activityTimeout = window.setTimeout(() => {
            // Touch session on backend
            touchSession();
            sessionState.update(s => ({ ...s, warningShown: false }));
        }, ACTIVITY_DEBOUNCE);
    };

    TRACKED_EVENTS.forEach(event => {
        document.addEventListener(event, activityHandler!, { passive: true });
    });
}

/**
 * Touch session to extend it
 */
async function touchSession(): Promise<void> {
    try {
        await tauriTouchSession();
    } catch (error) {
        // Failed to touch session
    }
}

/**
 * Logout user and redirect to login page
 */
export async function logout(): Promise<void> {
    try {
        await tauriLogout();
        setCurrentUser(null);
        showInfo('تم تسجيل خروجك بنجاح', 'تم تسجيل الخروج');
        push('/login');
    } catch (error) {
        // Still redirect even if backend call fails
        push('/login');
    } finally {
        stopSessionMonitoring();
    }
}

/**
 * Get current session info
 */
export function getSessionInfo(): { isActive: boolean; lastCheck: Date | null } {
    let state: SessionState | undefined;
    sessionState.subscribe(s => state = s)();
    return {
        isActive: state?.isActive ?? false,
        lastCheck: state?.lastCheck ?? null
    };
}

/**
 * Initialize session management
 * Call this when the app starts (after successful login)
 */
export function initSessionManagement(): void {
    startSessionMonitoring();
}

/**
 * Cleanup session management
 * Call this when logging out or app shutdown
 */
export function cleanupSessionManagement(): void {
    stopSessionMonitoring();
}
