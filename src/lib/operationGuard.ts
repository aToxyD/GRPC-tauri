import { writable, type Writable } from 'svelte/store';
import { formatErrorMessage } from './errors';
import { telemetry } from './telemetry';
import type { RuntimeScope } from './runtimeCleanup';

export interface OperationGuard {
  loading: Writable<boolean>;
  guard: (fn: () => Promise<void>) => Promise<void>;
}

export interface OperationGuardOptions {
  scope?: RuntimeScope;
}

/**
 * Creates a lightweight operation guard to prevent double-submits and race conditions.
 */
export function createOperationGuard(options?: OperationGuardOptions): OperationGuard {
  const scope = options?.scope;
  const loading = writable(false);

  async function guard(fn: () => Promise<void>): Promise<void> {
    if (scope && !scope.isAlive()) return;

    let currentlyLoading = false;
    loading.update((state) => {
      currentlyLoading = state;
      return true;
    });

    if (currentlyLoading) {
      telemetry.trackFailedOperation('guarded_operation', 'concurrent_blocked');
      return;
    }

    const endOp = telemetry.beginAsyncOperation('guarded_operation');
    try {
      await fn();
      if (scope && !scope.isAlive()) return;
    } catch (err) {
      if (scope && !scope.isAlive()) return;
      telemetry.trackError(err, 'OperationGuard.guard');
      telemetry.trackAsyncFailure('guarded_operation', formatErrorMessage(err));
      telemetry.trackFailedOperation('guarded_operation', formatErrorMessage(err));
      throw err;
    } finally {
      endOp();
      loading.set(false);
    }
  }

  return { loading, guard };
}

export interface OrchestratedOperation {
  loading: Writable<boolean>;
  error: Writable<string | null>;
  run: <T>(fn: () => Promise<T>) => Promise<T | null>;
}

/**
 * Creates a structured async operation handler with loading/error state.
 */
export function createOperation(options?: OperationGuardOptions): OrchestratedOperation {
  const scope = options?.scope;
  const loading = writable(false);
  const error = writable<string | null>(null);

  async function run<T>(fn: () => Promise<T>): Promise<T | null> {
    if (scope && !scope.isAlive()) return null;

    let busy = false;
    loading.update((state) => {
      busy = state;
      return true;
    });

    if (busy) {
      telemetry.trackFailedOperation('orchestrated_operation', 'concurrent_blocked');
      return null;
    }

    error.set(null);
    const endOp = telemetry.beginAsyncOperation('orchestrated_operation');

    try {
      const result = await fn();
      if (scope && !scope.isAlive()) {
        telemetry.trackAsyncFailure('orchestrated_operation', 'stale_after_complete');
        return null;
      }
      return result;
    } catch (err) {
      if (scope && !scope.isAlive()) return null;
      const msg = formatErrorMessage(err);
      error.set(msg);
      telemetry.trackError(err, 'OperationGuard.run');
      telemetry.trackAsyncFailure('orchestrated_operation', msg);
      telemetry.trackFailedOperation('orchestrated_operation', msg);
      return null;
    } finally {
      endOp();
      loading.set(false);
    }
  }

  return { loading, error, run };
}
