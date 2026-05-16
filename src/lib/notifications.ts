//! Notification System
//!
//! نظام إشعارات عصري للواجهة الأمامية
//!
//! # Features
//! - إشعارات متعددة الأنواع (success, error, warning, info)
//! - إغلاق تلقائي
//! - قوائم انتظار
//! - RTL support للعربية

import type { Notification } from './types';

class NotificationManager {
  private notifications: Notification[] = [];
  private listeners: ((notifications: Notification[]) => void)[] = [];
  private nextId = 1;

  // Subscribe to notifications changes
  subscribe(listener: (notifications: Notification[]) => void) {
    this.listeners.push(listener);
    listener(this.notifications);
    
    // Return unsubscribe function
    return () => {
      const index = this.listeners.indexOf(listener);
      if (index > -1) {
        this.listeners.splice(index, 1);
      }
    };
  }

  // Notify all listeners
  private notify() {
    this.listeners.forEach(listener => listener(this.notifications));
  }

  // Add a new notification
  add(notification: Omit<Notification, 'id' | 'timestamp'>): Notification {
    const newNotification: Notification = {
      ...notification,
      id: this.nextId.toString(),
      timestamp: new Date().toISOString(),
    };

    this.notifications.unshift(newNotification);
    this.nextId++;

    // Auto remove if specified
    if (notification.auto_close !== false) {
      setTimeout(() => {
        this.remove(newNotification.id);
      }, 5000); // 5 seconds default
    }

    this.notify();
    return newNotification;
  }

  // Remove a notification
  remove(id: string) {
    const index = this.notifications.findIndex(n => n.id === id);
    if (index > -1) {
      this.notifications.splice(index, 1);
      this.notify();
    }
  }

  // Clear all notifications
  clear() {
    this.notifications = [];
    this.notify();
  }

  // Convenience methods
  success(message: string, title?: string) {
    return this.add({
      type: 'success',
      title: title || 'نجاح',
      message,
      auto_close: true,
    });
  }

  error(message: string, title?: string) {
    return this.add({
      type: 'error',
      title: title || 'خطأ',
      message,
      auto_close: false, // Errors stay until manually closed
    });
  }

  warning(message: string, title?: string) {
    return this.add({
      type: 'warning',
      title: title || 'تحذير',
      message,
      auto_close: true,
    });
  }

  info(message: string, title?: string) {
    return this.add({
      type: 'info',
      title: title || 'معلومات',
      message,
      auto_close: true,
    });
  }

  // Get current notifications (read-only)
  getNotifications(): Notification[] {
    return [...this.notifications];
  }
}

// Singleton instance
export const notificationManager = new NotificationManager();

// Svelte store for reactive notifications
import { writable } from 'svelte/store';

export const notifications = writable<Notification[]>([]);

// Subscribe to manager and update store
notificationManager.subscribe((notifs) => {
  notifications.set(notifs);
});

// Export convenience functions
export const showSuccess = (message: string, title?: string) => notificationManager.success(message, title);
export const showError = (message: string, title?: string) => notificationManager.error(message, title);
export const showWarning = (message: string, title?: string) => notificationManager.warning(message, title);
export const showInfo = (message: string, title?: string) => notificationManager.info(message, title);
export const removeNotification = (id: string) => notificationManager.remove(id);
export const clearNotifications = () => notificationManager.clear();
