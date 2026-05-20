import { describe, it, expect } from 'vitest';
import { createAsyncGuard } from '../../lib/asyncLifecycle';
import { createRuntimeScope } from '../../lib/runtimeCleanup';

describe('createAsyncGuard', () => {
  it('rejects stale results after scope dispose', async () => {
    const scope = createRuntimeScope();
    const guard = createAsyncGuard(scope);

    let resolveFn: (v: number) => void;
    const pending = new Promise<number>((resolve) => {
      resolveFn = resolve;
    });

    const runPromise = guard.run(async () => pending);
    scope.dispose();
    resolveFn!(42);

    const result = await runPromise;
    expect(result).toBeNull();
  });

  it('returns result when scope is alive', async () => {
    const scope = createRuntimeScope();
    const guard = createAsyncGuard(scope);
    const result = await guard.run(async () => 7);
    expect(result).toBe(7);
    scope.dispose();
  });
});
