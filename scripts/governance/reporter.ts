import type { Violation, ExecutionMetrics } from "./types";

const colors = {
  red: "\x1b[31m",
  green: "\x1b[32m",
  yellow: "\x1b[33m",
  cyan: "\x1b[36m",
  reset: "\x1b[0m",
  bold: "\x1b[1m",
};

const INVARIANT_LABELS: Record<string, string> = {
  CONTRACT_BOUNDARY: "INVARIANT A — Contract Boundary",
  PROJECTION_INTEGRITY: "INVARIANT B — Projection Integrity",
  RUNTIME_SAFETY: "INVARIANT C — Runtime Safety",
  ARCHITECTURE_GRAPH: "INVARIANT D — Architecture Graph",
  GOVERNANCE_FREEZE: "META — Governance Freeze",
};

const INVARIANT_COLORS: Record<string, string> = {
  CONTRACT_BOUNDARY: colors.cyan,
  PROJECTION_INTEGRITY: colors.yellow,
  RUNTIME_SAFETY: colors.red,
  ARCHITECTURE_GRAPH: colors.cyan,
  GOVERNANCE_FREEZE: colors.red,
};

export function reportViolations(violations: Violation[], metrics?: ExecutionMetrics, verbose?: boolean): void {
  const errors = violations.filter((v) => v.severity === "ERROR");
  const warnings = violations.filter((v) => v.severity === "WARNING");

  // Group by invariant
  const byInvariant = new Map<string, Violation[]>();
  for (const v of violations) {
    if (!byInvariant.has(v.invariant)) byInvariant.set(v.invariant, []);
    byInvariant.get(v.invariant)!.push(v);
  }

  // Sort invariants by defined order
  const invariantOrder = [
    "CONTRACT_BOUNDARY",
    "PROJECTION_INTEGRITY",
    "RUNTIME_SAFETY",
    "ARCHITECTURE_GRAPH",
    "GOVERNANCE_FREEZE",
  ];

  for (const inv of invariantOrder) {
    const invViolations = byInvariant.get(inv);
    if (!invViolations || invViolations.length === 0) continue;

    const color = INVARIANT_COLORS[inv] || colors.cyan;
    const label = INVARIANT_LABELS[inv] || inv;
    console.log(`\n${color}${colors.bold}${label}${colors.reset}`);

    const sorted = [...invViolations].sort((a, b) => {
      if (a.file !== b.file) return a.file.localeCompare(b.file);
      return (a.line || 0) - (b.line || 0);
    });

    for (const v of sorted) {
      const sevColor = v.severity === "ERROR" ? colors.red : colors.yellow;
      const icon = v.severity === "ERROR" ? "❌" : "⚠️";
      const ruleTag = v.rule ? ` ${v.rule}` : "";
      console.log(`  ${icon} ${sevColor}${v.severity}${ruleTag}: ${v.message}${colors.reset}`);
      if (v.file) {
        const loc = v.line ? `:${v.line}` : "";
        console.log(`    ${v.file}${loc}`);
      }
    }
  }

  // Summary
  console.log("");

  if (errors.length === 0 && warnings.length === 0) {
    console.log(`✅ ${colors.green}${colors.bold}Architecture check passed with no issues.${colors.reset}`);
    if (verbose && metrics) {
      console.log(`\n${colors.cyan}Governance Execution Metrics:${colors.reset}`);
      console.log(`  Files scanned: ${metrics.filesScanned}`);
      console.log(`  Cache hits: ${metrics.cacheHits}`);
      console.log(`  Cache misses: ${metrics.cacheMisses}`);
      console.log(`  Total duration: ${metrics.invariantTimings.reduce((s, t) => s + t.durationMs, 0)}ms`);
      for (const t of metrics.invariantTimings) {
        console.log(`    ${t.name}: ${t.durationMs}ms`);
      }
    }
    process.exit(0);
  } else if (errors.length === 0) {
    console.log(`❌ ${colors.yellow}Architecture check failed: ${warnings.length} warning(s) require resolution (zero-warning policy).${colors.reset}`);
    if (verbose && metrics) {
      console.log(`\n${colors.cyan}Governance Execution Metrics:${colors.reset}`);
      console.log(`  Files scanned: ${metrics.filesScanned}`);
      console.log(`  Cache hits: ${metrics.cacheHits}`);
      console.log(`  Cache misses: ${metrics.cacheMisses}`);
      console.log(`  Total duration: ${metrics.invariantTimings.reduce((s, t) => s + t.durationMs, 0)}ms`);
      for (const t of metrics.invariantTimings) {
        console.log(`    ${t.name}: ${t.durationMs}ms`);
      }
    }
    process.exit(1);
  } else {
    console.log(`\n❌ ${colors.red}${errors.length} rule(s) violated. Architectural integrity compromised.${colors.reset}`);
    if (warnings.length > 0) {
      console.log(`⚠️  ${colors.yellow}${warnings.length} additional warning(s) require review.${colors.reset}`);
    }
    if (verbose && metrics) {
      console.log(`\n${colors.cyan}Governance Execution Metrics:${colors.reset}`);
      console.log(`  Files scanned: ${metrics.filesScanned}`);
      console.log(`  Cache hits: ${metrics.cacheHits}`);
      console.log(`  Cache misses: ${metrics.cacheMisses}`);
      console.log(`  Total duration: ${metrics.invariantTimings.reduce((s, t) => s + t.durationMs, 0)}ms`);
      for (const t of metrics.invariantTimings) {
        console.log(`    ${t.name}: ${t.durationMs}ms`);
      }
    }
    process.exit(1);
  }
}
