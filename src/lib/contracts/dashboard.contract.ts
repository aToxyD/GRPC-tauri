import { safeInvoke } from '../tauri';
import type { BuildInfo, TelemetryEvent } from '../types';

export async function getBuildInfo(): Promise<BuildInfo> {
  return await safeInvoke('get_build_info');
}

export async function getRecentTelemetry(limit: number): Promise<TelemetryEvent[]> {
  return await safeInvoke('get_recent_telemetry', { limit });
}
