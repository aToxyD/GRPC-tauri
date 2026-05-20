import { describe, it, expect, vi } from 'vitest';
import { get } from 'svelte/store';
import { createOperation, createOperationGuard } from '../../lib/operationGuard';
import { createRuntimeScope } from '../../lib/runtimeCleanup';

describe('operationGuard with runtime scope', () => {
  it('blocks concurrent operations', async () => {
    const scope = createRuntimeScope();
    const op = createOperation({ scope });
    let resolveFirst: () => void;
    const first = op.run(
      () =>
        new Promise<string>((resolve) => {
          resolveFirst = () => resolve('ok');
        })
    );
    const second = await op.run(async () => 'blocked');
    expect(second).toBeNull();
    resolveFirst!();
    await first;
    scope.dispose();
  });

  it('does not set loading after scope dispose', async () => {
    const scope = createRuntimeScope();
    const op = createOperation({ scope });
    let resolveFn: () => void;
    const pending = op.run(
      () =>
        new Promise<void>((resolve) => {
          resolveFn = resolve;
        })
    );
    scope.dispose();
    resolveFn!();
    await pending;
    expect(get(op.loading)).toBe(false);
  });

  it('guard prevents double submit', async () => {
    vi.useFakeTimers();
    const scope = createRuntimeScope();
    const { guard } = createOperationGuard({ scope });
    let calls = 0;
    const p1 = guard(async () => {
      calls++;
      await new Promise((r) => setTimeout(r, 50));
    });
    const p2 = guard(async () => {
      calls++;
    });
    await vi.advanceTimersByTimeAsync(60);
    await Promise.all([p1, p2]);
    expect(calls).toBe(1);
    scope.dispose();
    vi.useRealTimers();
  });
});
