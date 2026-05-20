/**
 * Defensive rendering helpers — null-safe display values.
 */

export function safeString(value: unknown, fallback = ''): string {
  if (value === null || value === undefined) return fallback;
  if (typeof value === 'string') return value;
  if (typeof value === 'number' || typeof value === 'boolean') return String(value);
  return fallback;
}

export function safeNumber(value: unknown, fallback = 0): number {
  if (typeof value === 'number' && !Number.isNaN(value)) return value;
  if (typeof value === 'string') {
    const parsed = Number(value);
    if (!Number.isNaN(parsed)) return parsed;
  }
  return fallback;
}

export function safeArray<T>(value: unknown): T[] {
  return Array.isArray(value) ? (value as T[]) : [];
}

export function safeOptional<T>(value: T | null | undefined, fallback: T): T {
  return value ?? fallback;
}

export function safeRecord<K extends string, V>(
  value: unknown
): Record<K, V> {
  if (value !== null && typeof value === 'object' && !Array.isArray(value)) {
    return value as Record<K, V>;
  }
  return {} as Record<K, V>;
}
