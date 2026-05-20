import { describe, it, expect } from 'vitest';
import { telemetry } from '../../lib/telemetry';

describe('TelemetrySystem', () => {
  it('prunes buffer at max size', () => {
    telemetry.clear();
    for (let i = 0; i < 1100; i++) {
      telemetry.trackOperation(`op_${i}`, 1);
    }
    const buffer = telemetry.getBuffer();
    expect(buffer.length).toBeLessThanOrEqual(1000);
  });

  it('aggregates summary metrics', () => {
    telemetry.clear();
    telemetry.trackOperation('test', 100);
    telemetry.trackFailedOperation('fail', 'reason');
    telemetry.trackNotification('shown', 'success');
    const summary = telemetry.getSummary();
    expect(summary.operationCount).toBeGreaterThan(0);
    expect(summary.notificationCount).toBeGreaterThan(0);
  });

  it('tracks async operation lifecycle', () => {
    telemetry.clear();
    const end = telemetry.beginAsyncOperation('lifecycle_test');
    expect(telemetry.getSummary().activeAsyncOperations).toBe(1);
    end();
    expect(telemetry.getSummary().activeAsyncOperations).toBe(0);
  });
});
