//! Notification System — unified bus with deterministic lifecycle

import type { Notification } from './types';
import { telemetry } from './telemetry';

const MAX_QUEUE = 10;
const AUTO_CLOSE_MS: Record<Notification['type'], number | null> = {
  success: 3000,
  warning: 5000,
  info: 5000,
  error: null,
  progress: null,
};

class NotificationManager {
  private notifications: Notification[] = [];
  private listeners: ((notifications: Notification[]) => void)[] = [];
  private nextId = 1;
  private dismissTimers = new Map<string, ReturnType<typeof setTimeout>>();
  private recentKeys = new Map<string, number>();
  private readonly DEDUP_WINDOW_MS = 2000;

  subscribe(listener: (notifications: Notification[]) => void) {
    this.listeners.push(listener);
    listener(this.notifications);

    return () => {
      const index = this.listeners.indexOf(listener);
      if (index > -1) {
        this.listeners.splice(index, 1);
      }
    };
  }

  private notify() {
    this.listeners.forEach((listener) => listener(this.notifications));
  }

  private cancelTimer(id: string) {
    const timer = this.dismissTimers.get(id);
    if (timer !== undefined) {
      clearTimeout(timer);
      this.dismissTimers.delete(id);
    }
  }

  private cancelAllTimers() {
    this.dismissTimers.forEach((timer) => clearTimeout(timer));
    this.dismissTimers.clear();
  }

  private scheduleAutoDismiss(notification: Notification) {
    if (notification.auto_close === false) return;

    const ms = AUTO_CLOSE_MS[notification.type];
    if (ms === null) return;

    const timer = setTimeout(() => {
      this.dismissTimers.delete(notification.id);
      this.remove(notification.id);
    }, ms);

    this.dismissTimers.set(notification.id, timer);
  }

  private isDuplicate(type: Notification['type'], message: string): boolean {
    const key = `${type}:${message}`;
    const last = this.recentKeys.get(key);
    const now = Date.now();
    if (last !== undefined && now - last < this.DEDUP_WINDOW_MS) {
      return true;
    }
    this.recentKeys.set(key, now);
    return false;
  }

  add(notification: Omit<Notification, 'id' | 'timestamp'>): Notification | null {
    if (this.isDuplicate(notification.type, notification.message)) {
      telemetry.trackNotification('deduped', notification.type);
      return null;
    }

    while (this.notifications.length >= MAX_QUEUE) {
      const oldest = this.notifications[this.notifications.length - 1];
      if (oldest) this.remove(oldest.id);
    }

    const newNotification: Notification = {
      ...notification,
      id: this.nextId.toString(),
      timestamp: new Date().toISOString(),
    };

    this.notifications.unshift(newNotification);
    this.nextId++;

    this.scheduleAutoDismiss(newNotification);
    telemetry.trackNotification('shown', newNotification.type);
    this.notify();
    return newNotification;
  }

  remove(id: string) {
    this.cancelTimer(id);
    const index = this.notifications.findIndex((n) => n.id === id);
    if (index > -1) {
      this.notifications.splice(index, 1);
      telemetry.trackNotification('dismissed');
      this.notify();
    }
  }

  clear() {
    this.cancelAllTimers();
    this.notifications = [];
    this.notify();
  }

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
      auto_close: false,
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

  progress(message: string, value?: number, title?: string) {
    return this.add({
      type: 'progress',
      title: title || 'جاري المعالجة',
      message,
      auto_close: false,
      progress_value: value,
    });
  }

  getNotifications(): Notification[] {
    return [...this.notifications];
  }
}

export const notificationManager = new NotificationManager();

import { writable } from 'svelte/store';

export const notifications = writable<Notification[]>([]);

notificationManager.subscribe((notifs) => {
  notifications.set(notifs);
});

export const showSuccess = (message: string, title?: string) =>
  notificationManager.success(message, title);
export const showError = (message: string, title?: string) =>
  notificationManager.error(message, title);
export const showWarning = (message: string, title?: string) =>
  notificationManager.warning(message, title);
export const showInfo = (message: string, title?: string) =>
  notificationManager.info(message, title);
export const showProgress = (message: string, value?: number, title?: string) =>
  notificationManager.progress(message, value, title);
export const removeNotification = (id: string) => notificationManager.remove(id);
export const clearNotifications = () => notificationManager.clear();
