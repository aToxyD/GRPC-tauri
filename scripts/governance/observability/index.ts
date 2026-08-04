/**
 * Phase 7 — Governance Observability & Self-Audit
 *
 * Generates informational reports about governance state without
 * introducing new enforcement, rules, or behavior changes.
 *
 * Usage: bun scripts/governance/observability/index.ts
 * Integration: check_arch.ts --observability
 */

import { writeFileSync, mkdirSync } from "fs";
import { FileCache } from "../scanner";
import { computeTelemetry, generateTelemetryReport, saveTelemetry } from "./telemetry";
import { computeHealthScore, generateHealthScoreReport } from "./healthScore";
import { runSelfAudit, generateSelfAuditReport } from "./selfAudit";
import { analyzeChangeImpact, generateChangeImpactReport } from "./changeImpact";
import { generateTimelineReport } from "./timeline";
import { detectDeadArtifacts, generateDeadArtifactsReport } from "./deadArtifacts";
import { collectEngineMetrics, generateEngineMetricsReport } from "./engineMetrics";
import { computeCoverageReport, generateCoverageReport } from "./coverageReport";
import type { ExecutionMetrics } from "../types";

const DOCS_DIR = "docs/governance/frontend";
// We do NOT want to create a .governance-telemetry.json file for state tracking.
// All reports are generated fresh from current state.

function ensureDocsDir(): void {
  mkdirSync(DOCS_DIR, { recursive: true });
}

export function runObservability(
  engineMetrics?: ExecutionMetrics | null,
  violations?: number,
  warnings?: number,
): {
  telemetryFile: string;
  healthScoreFile: string;
  selfAuditFile: string;
  changeImpactFile: string;
  timelineFile: string;
  deadArtifactsFile: string;
  engineMetricsFile: string;
  coverageReportFile: string;
} {
  const start = performance.now();
  const cache = new FileCache();

  ensureDocsDir();

  // Compute all observability data
  const telemetryData = computeTelemetry(cache);
  saveTelemetry(telemetryData);
  const healthScoreData = computeHealthScore(cache);
  const selfAuditItems = runSelfAudit(cache);
  const changeImpacts = analyzeChangeImpact();
  const timelineReport = generateTimelineReport();
  const deadArtifacts = detectDeadArtifacts(cache);
  const engineMetricsData = collectEngineMetrics(
    engineMetrics ?? null,
    violations ?? 0,
    warnings ?? 0,
    start,
  );
  const coverageReportData = computeCoverageReport(cache);

  // Generate reports
  const telemetryReport = generateTelemetryReport(telemetryData);
  const healthScoreReport = generateHealthScoreReport(healthScoreData);
  const selfAuditReport = generateSelfAuditReport(selfAuditItems);
  const changeImpactReport = generateChangeImpactReport(changeImpacts);
  const deadArtifactsReport = generateDeadArtifactsReport(deadArtifacts);
  const engineMetricsReport = generateEngineMetricsReport(engineMetricsData);
  const coverageReport = generateCoverageReport(coverageReportData);

  // Write files
  const telemetryFile = `${DOCS_DIR}/GOVERNANCE_TELEMETRY.md`;
  const healthScoreFile = `${DOCS_DIR}/GOVERNANCE_HEALTH_SCORE.md`;
  const selfAuditFile = `${DOCS_DIR}/SELF_AUDIT.md`;
  const changeImpactFile = `${DOCS_DIR}/CHANGE_IMPACT.md`;
  const timelineFile = `${DOCS_DIR}/GOVERNANCE_TIMELINE.md`;
  const deadArtifactsFile = `${DOCS_DIR}/DEAD_ARTIFACTS.md`;
  const engineMetricsFile = `${DOCS_DIR}/GOVERNANCE_ENGINE_METRICS.md`;
  const coverageReportFile = `${DOCS_DIR}/GOVERNANCE_COVERAGE_REPORT.md`;

  writeFileSync(telemetryFile, telemetryReport);
  writeFileSync(healthScoreFile, healthScoreReport);
  writeFileSync(selfAuditFile, selfAuditReport);
  writeFileSync(changeImpactFile, changeImpactReport);
  writeFileSync(timelineFile, timelineReport);
  writeFileSync(deadArtifactsFile, deadArtifactsReport);
  writeFileSync(engineMetricsFile, engineMetricsReport);
  writeFileSync(coverageReportFile, coverageReport);

  console.log("\n\x1b[36m\x1b[1mGovernance Observability Reports Generated:\x1b[0m");
  console.log(`  📊 ${telemetryFile}`);
  console.log(`  🏥 ${healthScoreFile}`);
  console.log(`  🔍 ${selfAuditFile}`);
  console.log(`  🔄 ${changeImpactFile}`);
  console.log(`  📅 ${timelineFile}`);
  console.log(`  💀 ${deadArtifactsFile}`);
  console.log(`  ⚡ ${engineMetricsFile}`);
  console.log(`  📊 ${coverageReportFile}`);

  return {
    telemetryFile,
    healthScoreFile,
    selfAuditFile,
    changeImpactFile,
    timelineFile,
    deadArtifactsFile,
    engineMetricsFile,
    coverageReportFile,
  };
}

// Allow running standalone
if (process.argv[1]?.endsWith("index.ts")) {
  runObservability(null, 0, 0);
  process.exit(0);
}
