import { writable, type Writable } from 'svelte/store';
import { formatErrorMessage } from './errors';
import { telemetry } from './telemetry';

export interface OperationGuard {
  loading: Writable<boolean>;
  guard: (fn: () => Promise<void>) => Promise<void>;
}

/**
 * Creates a lightweight operation guard to prevent double-submits and race conditions.
 * Returns a writable store for loading state and a guard function to wrap async actions.
 */
export function createOperationGuard(): OperationGuard {
  const loading = writable(false);

  async function guard(fn: () => Promise<void>): Promise<void> {
    let currentlyLoading = false;
    loading.update(state => {
      currentlyLoading = state;
      return true;
    });

    if (currentlyLoading) {
      console.warn('[OperationGuard] Operation already in progress. Blocking concurrent execution.');
      return;
    }

    const startTime = performance.now();
    try {
      await fn();
      telemetry.trackOperation('guarded_operation', performance.now() - startTime);
    } catch (err) {
      telemetry.trackError(err, 'OperationGuard.guard');
      telemetry.trackAsyncFailure('guarded_operation', formatErrorMessage(err));
      throw err;
    } finally {
      loading.set(false);
    }
  }

  return {
    loading,
    guard,
  };
}

export interface OrchestratedOperation {
  loading: Writable<boolean>;
  error: Writable<string | null>;
  run: <T>(fn: () => Promise<T>) => Promise<T | null>;
}

/**
 * Creates a structured async operation handler that manages loading, error states,
 * and try-catch-finally normalization in a single place.
 */
export function createOperation(): OrchestratedOperation {
  const loading = writable(false);
  const error = writable<string | null>(null);

  async function run<T>(fn: () => Promise<T>): Promise<T | null> {
    let busy = false;
    loading.update(state => {
      busy = state;
      return true;
    });

    if (busy) {
      console.warn('[OperationGuard] Operation blocked: another task is active.');
      return null;
    }

    error.set(null);
    const startTime = performance.now();
    try {
      const result = await fn();
      telemetry.trackOperation('orchestrated_operation', performance.now() - startTime);
      return result;
    } catch (err) {
      const msg = formatErrorMessage(err);
      error.set(msg);
      telemetry.trackError(err, 'OperationGuard.run');
      telemetry.trackAsyncFailure('orchestrated_operation', msg);
      return null;
    } finally {
      loading.set(false);
    }
  }

  return {
    loading,
    error,
    run,
  };
}
