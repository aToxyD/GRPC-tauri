import { FileCache } from "./scanner";
import type { Violation, ExecutionMetrics, InvariantTiming } from "./types";
import { reportViolations } from "./reporter";
import { scanContractBoundary } from "./invariants/contractBoundary";
import { scanProjectionIntegrity } from "./invariants/projectionIntegrity";
import { scanRuntimeSafety } from "./invariants/runtimeSafety";
import { scanArchitectureGraph } from "./invariants/architectureGraph";
import { scanGovernanceFreeze } from "./invariants/governanceFreeze";
import { runObservability } from "./observability/index";

const VERBOSE = process.argv.includes("--verbose");
const OBSERVABILITY = process.argv.includes("--observability") || process.argv.includes("--obs");

interface Scanner {
  name: string;
  scan: (cache: FileCache) => Violation[];
}

export function runGovernanceAudit(): void {
  const cache = new FileCache();
  const allViolations: Violation[] = [];
  const timings: InvariantTiming[] = [];

  const scanners: Scanner[] = [
    { name: "CONTRACT_BOUNDARY", scan: scanContractBoundary },
    { name: "PROJECTION_INTEGRITY", scan: scanProjectionIntegrity },
    { name: "RUNTIME_SAFETY", scan: scanRuntimeSafety },
    { name: "ARCHITECTURE_GRAPH", scan: scanArchitectureGraph },
    { name: "GOVERNANCE_FREEZE", scan: scanGovernanceFreeze },
  ];

  console.log("\x1b[1mRunning Modular Governance Engine...\x1b[0m\n");

  for (const { name, scan } of scanners) {
    const start = performance.now();
    const violations = scan(cache);
    allViolations.push(...violations);
    timings.push({ name, durationMs: Math.round(performance.now() - start) });
  }

  const metrics: ExecutionMetrics = {
    filesScanned: cache.files,
    cacheHits: cache.hits,
    cacheMisses: cache.misses,
    invariantTimings: timings,
  };

  // Run observability reports before reporting violations (informational only)
  if (OBSERVABILITY) {
    try {
      const errorCount = allViolations.filter((v) => v.severity === "ERROR").length;
      const warningCount = allViolations.filter((v) => v.severity === "WARNING").length;
      runObservability(metrics, errorCount, warningCount);
    } catch (e) {
      console.error("  ⚠️  Observability report generation failed (non-blocking):", e);
    }
  }

  reportViolations(allViolations, metrics, VERBOSE);
}
