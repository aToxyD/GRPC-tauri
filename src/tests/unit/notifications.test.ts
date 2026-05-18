import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { get } from 'svelte/store';
import { 
    notificationManager, 
    notifications, 
    showSuccess, 
    showError, 
    showWarning, 
    showInfo, 
    removeNotification, 
    clearNotifications 
} from '../../lib/notifications';

describe('Notification System', () => {
    beforeEach(() => {
        vi.useFakeTimers();
        clearNotifications();
    });

    afterEach(() => {
        vi.useRealTimers();
    });

    it('should add a success notification and auto-close it after 5 seconds', () => {
        showSuccess('عملية ناجحة', 'نجاح');
        
        let list = get(notifications);
        expect(list.length).toBe(1);
        expect(list[0].type).toBe('success');
        expect(list[0].message).toBe('عملية ناجحة');
        expect(list[0].title).toBe('نجاح');
        expect(list[0].auto_close).toBe(true);

        // Fast-forward time to check auto-close
        vi.advanceTimersByTime(5000);
        expect(get(notifications).length).toBe(0);
    });

    it('should add an error notification and NOT auto-close it', () => {
        showError('حدث خطأ فادح', 'خطأ');
        
        let list = get(notifications);
        expect(list.length).toBe(1);
        expect(list[0].type).toBe('error');
        expect(list[0].auto_close).toBe(false);

        // Fast-forward time and check it's still there
        vi.advanceTimersByTime(10000);
        expect(get(notifications).length).toBe(1);
    });

    it('should support warnings and info notifications', () => {
        showWarning('هذا تحذير');
        showInfo('هذه معلومة');

        const list = get(notifications);
        expect(list.length).toBe(2);
        expect(list[0].type).toBe('info');
        expect(list[1].type).toBe('warning');
    });

    it('should allow manually removing a notification by ID', () => {
        const notif = showSuccess('سيتم الحذف');
        expect(get(notifications).length).toBe(1);

        removeNotification(notif.id);
        expect(get(notifications).length).toBe(0);
    });

    it('should allow clearing all notifications', () => {
        showSuccess('1');
        showSuccess('2');
        showSuccess('3');
        expect(get(notifications).length).toBe(3);

        clearNotifications();
        expect(get(notifications).length).toBe(0);
    });
});
