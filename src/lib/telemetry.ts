export interface TelemetryRecord {
  type: 'operation' | 'error' | 'latency' | 'page_load' | 'async_failure';
  name: string;
  duration?: number;
  error?: Error | unknown;
  context?: string;
  details?: unknown;
  timestamp: string;
}

class TelemetrySystem {
  private buffer: TelemetryRecord[] = [];
  private readonly MAX_BUFFER_SIZE = 1000;
  private debug = import.meta.env?.DEV || false;

  private pushRecord(record: TelemetryRecord) {
    if (this.buffer.length >= this.MAX_BUFFER_SIZE) {
      // Cleanup oldest records to prevent memory leak
      this.buffer.shift();
    }
    this.buffer.push(record);

    if (this.debug) {
      console.debug(`[Telemetry: ${record.type}]`, record);
    }
  }

  trackOperation(name: string, duration: number, details?: unknown) {
    this.pushRecord({
      type: 'operation',
      name,
      duration,
      details,
      timestamp: new Date().toISOString()
    });
  }

  trackError(error: Error | unknown, context?: string) {
    this.pushRecord({
      type: 'error',
      name: 'error',
      error,
      context,
      timestamp: new Date().toISOString()
    });
  }

  trackLatency(operation: string, latencyMs: number) {
    this.pushRecord({
      type: 'latency',
      name: operation,
      duration: latencyMs,
      timestamp: new Date().toISOString()
    });
  }

  trackPageLoad(page: string, duration: number) {
    this.pushRecord({
      type: 'page_load',
      name: page,
      duration,
      timestamp: new Date().toISOString()
    });
  }

  trackAsyncFailure(operation: string, reason: string) {
    this.pushRecord({
      type: 'async_failure',
      name: operation,
      context: reason,
      timestamp: new Date().toISOString()
    });
  }

  clear() {
    this.buffer = [];
  }
  
  getBuffer() {
    return [...this.buffer];
  }
}

export const telemetry = new TelemetrySystem();
