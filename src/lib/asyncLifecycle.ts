/**
 * Async lifecycle safety — cancellation, stale-response rejection, mount guards.
 */

import type { RuntimeScope } from './runtimeCleanup';
import { telemetry } from './telemetry';

export interface AsyncGuard {
  readonly signal: AbortSignal;
  isAlive(): boolean;
  run<T>(fn: (signal: AbortSignal) => Promise<T>): Promise<T | null>;
}

/**
 * Creates an async guard bound to a runtime scope (or standalone AbortController).
 */
export function createAsyncGuard(scope?: RuntimeScope): AsyncGuard {
  const controller = new AbortController();
  const signal = scope?.signal ?? controller.signal;

  return {
    get signal() {
      return signal;
    },

    isAlive(): boolean {
      if (signal.aborted) return false;
      return scope ? scope.isAlive() : true;
    },

    async run<T>(fn: (signal: AbortSignal) => Promise<T>): Promise<T | null> {
      if (!this.isAlive()) {
        telemetry.trackAsyncFailure('async_guard', 'aborted_before_start');
        return null;
      }

      const generation = Date.now();
      const opId = `async_${generation}`;

      try {
        const result = await fn(signal);
        if (!this.isAlive() || signal.aborted) {
          telemetry.trackAsyncFailure(opId, 'stale_response_rejected');
          return null;
        }
        return result;
      } catch (err) {
        if (signal.aborted || !this.isAlive()) {
          telemetry.trackAsyncFailure(opId, 'cancelled_during_execution');
          return null;
        }
        throw err;
      }
    },
  };
}

/**
 * Wraps a promise so its result is ignored if the guard is no longer alive.
 */
export async function withStaleGuard<T>(
  guard: AsyncGuard,
  promise: Promise<T>
): Promise<T | null> {
  const result = await promise;
  if (!guard.isAlive()) {
    telemetry.trackAsyncFailure('stale_guard', 'post_await_rejected');
    return null;
  }
  return result;
}
