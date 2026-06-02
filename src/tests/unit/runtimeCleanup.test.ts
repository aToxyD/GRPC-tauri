import { describe, it, expect } from 'vitest';
import { createRuntimeScope, createTransientMessage, getCleanupMetrics } from '../../lib/runtimeCleanup';
import { DisposableStack } from '../../lib/disposables';

describe('DisposableStack', () => {
  it('runs disposables in reverse order', () => {
    const order: number[] = [];
    const stack = new DisposableStack();
    stack.add(() => order.push(1));
    stack.add(() => order.push(2));
    stack.dispose();
    expect(order).toEqual([2, 1]);
  });

  it('aborts controller on dispose', () => {
    const stack = new DisposableStack();
    const controller = new AbortController();
    const signal = stack.addAbortController(controller);
    expect(signal.aborted).toBe(false);
    stack.dispose();
    expect(signal.aborted).toBe(true);
  });
});

describe('createRuntimeScope', () => {
  it('clears timers on dispose', () => {
    const scope = createRuntimeScope();
    let called = false;
    scope.setTimeout(() => { called = true; }, 1000);
    scope.dispose();
    expect(called).toBe(false);
  });

  it('reports cleanup metrics', () => {
    const scope = createRuntimeScope();
    scope.setTimeout(() => {}, 10);
    scope.dispose();
    const metrics = getCleanupMetrics();
    expect(metrics.disposeCalls).toBeGreaterThan(0);
  });
});

describe('createTransientMessage', () => {
  it('clears message after duration', () => {
    const scope = createRuntimeScope();
    let msg = 'hello';
    const setMsg = createTransientMessage(scope, (m) => (msg = m), 0);
    setMsg('done');
    expect(msg).toBe('done');
    scope.dispose();
  });
});
