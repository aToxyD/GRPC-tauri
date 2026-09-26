import { describe, it, expect, beforeEach, vi } from 'vitest';

// Mock the Tauri IPC bridge. Every contract function funnels through
// `safeInvoke` -> `invoke` (tauri.ts) — the only permitted boundary.
const mockInvoke = vi.fn();
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
}));

import {
  exportContractFulfillmentPackage,
  importContractFulfillmentPackage,
} from '../../../lib/contracts/sync.contract';

/** Find the args object passed to `invoke` for a given command. */
function argsFor(command: string): Record<string, unknown> {
  const call = mockInvoke.mock.calls.find(([name]) => name === command);
  expect(call, `expected invoke('${command}', ...) to have been called`).toBeDefined();
  return (call![1] ?? {}) as Record<string, unknown>;
}

describe('ADR-0061 contract_fulfillment IPC contract', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('exports with exactly the filePath argument (dataset is backend-derived)', async () => {
    mockInvoke.mockResolvedValue({ success: true, record_count: 3 });

    const result = await exportContractFulfillmentPackage('/tmp/fulfillment.sync');

    // A package_id is not an input: the backend derives the dataset AND its
    // content-derived identity, so passing one would let the renderer forge
    // the idempotency identity.
    expect(Object.keys(argsFor('export_contract_fulfillment_package')).sort()).toEqual(['filePath']);
    expect(result.record_count).toBe(3);
  });

  it('imports with exactly filePath and unitId', async () => {
    mockInvoke.mockResolvedValue({
      applied_count: 2,
      already_satisfied_count: 1,
      unit_id: 'unit-a',
      package_id: 'abc123',
      imported_by: 'admin',
      timestamp: '2026-09-26T00:00:00Z',
    });

    const result = await importContractFulfillmentPackage('/tmp/fulfillment.sync', 'unit-a');

    expect(Object.keys(argsFor('import_contract_fulfillment_package')).sort()).toEqual([
      'filePath',
      'unitId',
    ]);
    // Both dispositions are reported verbatim by the backend; the renderer never
    // derives them (A5/F3).
    expect(result.applied_count).toBe(2);
    expect(result.already_satisfied_count).toBe(1);
  });

  it('propagates backend errors instead of masking them', async () => {
    mockInvoke.mockRejectedValue('الحزمة مكررة');

    await expect(importContractFulfillmentPackage('/tmp/fulfillment.sync', 'unit-a')).rejects.toBeTruthy();
  });
});
