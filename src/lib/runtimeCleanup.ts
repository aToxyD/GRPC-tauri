/**
 * Centralized runtime cleanup — tracked timers, intervals, and listeners.
 */

import { DisposableStack } from './disposables';
import { telemetry } from './telemetry';

export interface RuntimeScope {
  readonly signal: AbortSignal;
  isAlive(): boolean;
  setTimeout(fn: () => void, ms: number): number;
  setInterval(fn: () => void, ms: number): number;
  addListener<K extends keyof WindowEventMap>(
    target: Window | Document | HTMLElement,
    event: K,
    handler: (ev: WindowEventMap[K]) => void,
    options?: boolean | AddEventListenerOptions
  ): void;
  dispose(): void;
}

// @category UiState — telemetry counters
let globalTimerCount = 0;
// @category UiState — telemetry counters
let globalIntervalCount = 0;
// @category UiState — telemetry counters
let globalListenerCount = 0;
// @category UiState — telemetry counters
let globalDisposeCount = 0;

export function getCleanupMetrics() {
  return {
    activeTimers: globalTimerCount,
    activeIntervals: globalIntervalCount,
    activeListeners: globalListenerCount,
    disposeCalls: globalDisposeCount,
  };
}

/**
 * Creates a lifecycle-bound runtime scope. Call dispose() in onDestroy.
 */
export function createRuntimeScope(): RuntimeScope {
  const stack = new DisposableStack();
  const controller = new AbortController();
  stack.addAbortController(controller);

  const timeouts = new Set<number>();
  const intervals = new Set<number>();

  function trackTimeout(id: number) {
    timeouts.add(id);
    globalTimerCount++;
    telemetry.trackCleanup('timer_register');
    stack.add(() => {
      clearTimeout(id);
      timeouts.delete(id);
      globalTimerCount = Math.max(0, globalTimerCount - 1);
      telemetry.trackCleanup('timer_clear');
    });
    return id;
  }

  function trackInterval(id: number) {
    intervals.add(id);
    globalIntervalCount++;
    telemetry.trackCleanup('interval_register');
    stack.add(() => {
      clearInterval(id);
      intervals.delete(id);
      globalIntervalCount = Math.max(0, globalIntervalCount - 1);
      telemetry.trackCleanup('interval_clear');
    });
    return id;
  }

  return {
    get signal() {
      return controller.signal;
    },

    isAlive(): boolean {
      return !stack.isDisposed() && !controller.signal.aborted;
    },

    setTimeout(fn: () => void, ms: number): number {
      const id = window.setTimeout(() => {
        if (!this.isAlive()) return;
        try {
          fn();
        } catch (err) {
          telemetry.trackError(err, 'RuntimeScope.setTimeout');
        }
      }, ms);
      return trackTimeout(id);
    },

    setInterval(fn: () => void, ms: number): number {
      const id = window.setInterval(() => {
        if (!this.isAlive()) return;
        try {
          fn();
        } catch (err) {
          telemetry.trackError(err, 'RuntimeScope.setInterval');
        }
      }, ms);
      return trackInterval(id);
    },

    addListener(target, event, handler, options?) {
      const wrapped = ((ev: Event) => {
        if (!this.isAlive()) return;
        try {
          (handler as (e: Event) => void)(ev);
        } catch (err) {
          telemetry.trackError(err, `RuntimeScope.listener:${String(event)}`);
        }
      }) as typeof handler;

      target.addEventListener(event, wrapped as EventListener, options);
      globalListenerCount++;
      telemetry.trackCleanup('listener_register');

      stack.add(() => {
        target.removeEventListener(event, wrapped as EventListener, options);
        globalListenerCount = Math.max(0, globalListenerCount - 1);
        telemetry.trackCleanup('listener_remove');
      });
    },

    dispose(): void {
      globalDisposeCount++;
      stack.dispose();
      timeouts.forEach(clearTimeout);
      intervals.forEach(clearInterval);
      timeouts.clear();
      intervals.clear();
    },
  };
}

/**
 * Transient inline message helper (replaces per-page timeout arrays).
 */
export function createTransientMessage(
  scope: RuntimeScope,
  setter: (msg: string) => void,
  durationMs = 3000
): (msg: string) => void {
  return (msg: string) => {
    setter(msg);
    scope.setTimeout(() => {
      if (scope.isAlive()) setter('');
    }, durationMs);
  };
}
