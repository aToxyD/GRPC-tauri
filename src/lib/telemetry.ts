export interface TelemetryRecord {
  type:
    | 'operation'
    | 'error'
    | 'latency'
    | 'page_load'
    | 'async_failure'
    | 'cleanup'
    | 'notification'
    | 'unhandled_runtime';
  name: string;
  duration?: number;
  error?: Error | unknown;
  context?: string;
  details?: unknown;
  timestamp: string;
}

export interface TelemetrySummary {
  operationCount: number;
  errorCount: number;
  asyncFailureCount: number;
  unhandledRuntimeCount: number;
  notificationCount: number;
  cleanupCount: number;
  avgOperationDuration: number;
  activeAsyncOperations: number;
}

class TelemetrySystem {
  private buffer: TelemetryRecord[] = [];
  private readonly MAX_BUFFER_SIZE = 1000;
  private debug = import.meta.env?.DEV || false;

  private operationDurations: number[] = [];
  private failedOperationCount = 0;
  private unhandledRuntimeCount = 0;
  private notificationMetrics = { shown: 0, dismissed: 0, deduped: 0 };
  private cleanupMetrics = { timer_register: 0, timer_clear: 0, interval_register: 0, interval_clear: 0, listener_register: 0, listener_remove: 0 };
  private activeAsyncOperations = 0;

  private pushRecord(record: TelemetryRecord) {
    if (this.buffer.length >= this.MAX_BUFFER_SIZE) {
      this.buffer.shift();
    }
    this.buffer.push(record);

    if (this.debug) {
      console.debug(`[Telemetry: ${record.type}]`, record);
    }
  }

  trackOperation(name: string, duration: number, details?: unknown) {
    this.operationDurations.push(duration);
    if (this.operationDurations.length > 500) {
      this.operationDurations.shift();
    }
    this.pushRecord({
      type: 'operation',
      name,
      duration,
      details,
      timestamp: new Date().toISOString(),
    });
  }

  trackFailedOperation(name: string, reason: string) {
    this.failedOperationCount++;
    this.pushRecord({
      type: 'operation',
      name: `failed:${name}`,
      context: reason,
      timestamp: new Date().toISOString(),
    });
  }

  trackError(error: Error | unknown, context?: string) {
    this.pushRecord({
      type: 'error',
      name: 'error',
      error,
      context,
      timestamp: new Date().toISOString(),
    });
  }

  trackLatency(operation: string, latencyMs: number) {
    this.pushRecord({
      type: 'latency',
      name: operation,
      duration: latencyMs,
      timestamp: new Date().toISOString(),
    });
  }

  trackPageLoad(page: string, duration: number) {
    this.pushRecord({
      type: 'page_load',
      name: page,
      duration,
      timestamp: new Date().toISOString(),
    });
  }

  trackAsyncFailure(operation: string, reason: string) {
    this.pushRecord({
      type: 'async_failure',
      name: operation,
      context: reason,
      timestamp: new Date().toISOString(),
    });
  }

  trackUnhandledRuntime(source: string, message: string) {
    this.unhandledRuntimeCount++;
    this.pushRecord({
      type: 'unhandled_runtime',
      name: source,
      context: message,
      timestamp: new Date().toISOString(),
    });
  }

  trackCleanup(action: keyof typeof this.cleanupMetrics) {
    this.cleanupMetrics[action]++;
    this.pushRecord({
      type: 'cleanup',
      name: action,
      timestamp: new Date().toISOString(),
    });
  }

  trackNotification(action: 'shown' | 'dismissed' | 'deduped', type?: string) {
    this.notificationMetrics[action]++;
    this.pushRecord({
      type: 'notification',
      name: action,
      context: type,
      timestamp: new Date().toISOString(),
    });
  }

  beginAsyncOperation(name: string): () => void {
    this.activeAsyncOperations++;
    const start = performance.now();
    return () => {
      this.activeAsyncOperations = Math.max(0, this.activeAsyncOperations - 1);
      this.trackOperation(name, performance.now() - start);
    };
  }

  getSummary(): TelemetrySummary {
    const durations = this.operationDurations;
    // [arch:allow-fe146] see ADR-0054 — Reason: telemetry latency average (infrastructure metric, not business logic); Date: 2026-08-30; Owner: governance-team
    const avg =
      durations.length > 0
        ? durations.reduce((a, b) => a + b, 0) / durations.length
        : 0;

    return {
      operationCount: durations.length,
      errorCount: this.buffer.filter((r) => r.type === 'error').length,
      asyncFailureCount: this.buffer.filter((r) => r.type === 'async_failure').length,
      unhandledRuntimeCount: this.unhandledRuntimeCount,
      notificationCount: this.notificationMetrics.shown,
      cleanupCount: Object.values(this.cleanupMetrics).reduce((a, b) => a + b, 0),
      avgOperationDuration: avg,
      activeAsyncOperations: this.activeAsyncOperations,
    };
  }

  getNotificationMetrics() {
    return { ...this.notificationMetrics };
  }

  getCleanupMetrics() {
    return { ...this.cleanupMetrics };
  }

  clear() {
    this.buffer = [];
    this.operationDurations = [];
    this.failedOperationCount = 0;
    this.unhandledRuntimeCount = 0;
    this.notificationMetrics = { shown: 0, dismissed: 0, deduped: 0 };
    this.cleanupMetrics = {
      timer_register: 0,
      timer_clear: 0,
      interval_register: 0,
      interval_clear: 0,
      listener_register: 0,
      listener_remove: 0,
    };
    this.activeAsyncOperations = 0;
  }

  getBuffer() {
    return [...this.buffer];
  }
}

export const telemetry = new TelemetrySystem();
