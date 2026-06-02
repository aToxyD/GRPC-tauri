import type { ExecutionMetrics } from "../types";
import type { EngineMetricsData } from "./types";

export function collectEngineMetrics(
  metrics: ExecutionMetrics | null,
  violations: number,
  warnings: number,
  generationStart: number,
): EngineMetricsData {
  const generationTimeMs = Math.round(performance.now() - generationStart);

  return {
    filesScanned: metrics?.filesScanned ?? 0,
    cacheHits: metrics?.cacheHits ?? 0,
    cacheMisses: metrics?.cacheMisses ?? 0,
    violations,
    warnings,
    generationTimeMs,
  };
}

export function generateEngineMetricsReport(data: EngineMetricsData): string {
  const totalRequests = data.cacheHits + data.cacheMisses;
  const hitRate = totalRequests > 0
    ? `${Math.round((data.cacheHits / totalRequests) * 100)}%`
    : "—";

  const lines: string[] = [
    "# Governance Engine Metrics",
    "",
    `Generated: ${new Date().toISOString()}`,
    "",
    "---",
    "",
    "## Performance Metrics",
    "",
    "| Metric | Value |",
    "|--------|-------|",
    `| Files scanned | ${data.filesScanned} |`,
    `| Cache hits | ${data.cacheHits} |`,
    `| Cache misses | ${data.cacheMisses} |`,
    `| Cache hit rate | ${hitRate} |`,
    `| Violations found | ${data.violations} |`,
    `| Warnings found | ${data.warnings} |`,
    `| Report generation time | ${data.generationTimeMs}ms |`,
    "",
    "---",
    "",
    "## Performance Analysis",
    "",
    hitRate !== "—"
      ? `Cache hit rate of ${hitRate} indicates ${
          parseInt(hitRate) >= 80
            ? "efficient file reuse across invariant scanners."
            : "potential for improved file reuse. Consider reducing redundant file reads."
        }`
      : "No cache activity recorded.",
    "",
    data.generationTimeMs > 1000
      ? `Report generation at ${data.generationTimeMs}ms — consider sequential optimization if this becomes a bottleneck.`
      : `Report generation completed in ${data.generationTimeMs}ms — within acceptable range.`,
    "",
    "---",
    "",
    "## Observability Note",
    "",
    "This report is informational only. No enforcement. No build failure.",
    "",
  ];

  return lines.join("\n");
}
