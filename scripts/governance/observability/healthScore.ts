import { Glob } from "bun";
import { readFileSync } from "fs";
import type { FileCache } from "../scanner";
import { loadSnapshot } from "../snapshot";
import type { HealthScoreData } from "./types";

const SNAPSHOT_DIR = "docs/governance/frontend/baselines";

export function computeHealthScore(cache: FileCache): HealthScoreData {
  let deduction = 0;

  // Suppression deduction: -2 per active suppression
  const suppressionCount = countSuppressions(cache);
  deduction += suppressionCount * 2;

  // Stale approval deduction: check for approvals > 90 days
  const staleApprovals = countStaleApprovals();
  deduction += staleApprovals * 5;

  // Snapshot drift deduction: check for missing snapshots or invalid structure
  const driftCount = countSnapshotDrift(cache);
  deduction += driftCount * 5;

  // Orphan file deduction: pages not in registry or registry not matching pages
  const orphanCount = countOrphanFiles(cache);
  deduction += orphanCount * 3;

  // Ownership exceptions (cross-domain)
  const exceptionCount = countOwnershipExceptions();
  deduction += Math.floor(exceptionCount / 2);

  // Expired suppressions: -5 each
  const expiredCount = countExpiredSuppressions(cache);
  deduction += expiredCount * 5;

  const maxDeduction = 100;
  deduction = Math.min(deduction, maxDeduction);

  const score = 100 - deduction;

  // Per-invariant sub-scores (these are informational approximations)
  const architectureGraph = computeSubScore(cache, deduction, [
    "src/pages/**/*.svelte", "src/lib/contracts/*.contract.ts",
  ]);
  const runtimeSafety = computeSubScore(cache, deduction, ["src/**/*.{svelte,ts,js}"]);
  const contractBoundary = computeSubScore(cache, deduction, ["src/lib/contracts/*.contract.ts"]);
  const projectionIntegrity = computeSubScore(cache, deduction, [
    "src/pages/**/*.svelte", "src/components/**/*.svelte",
  ]);
  const governanceFreeze = computeSubScore(cache, deduction, []);

  return {
    architectureGraph: Math.max(0, Math.min(100, 100 - Math.floor(deduction * 0.8))),
    runtimeSafety: Math.max(0, Math.min(100, 100 - Math.floor(deduction * 0.6))),
    contractBoundary: Math.max(0, Math.min(100, 100 - Math.floor(deduction * 0.5))),
    projectionIntegrity: Math.max(0, Math.min(100, 100 - Math.floor(deduction * 0.7))),
    governanceFreeze: Math.max(0, Math.min(100, 100 - Math.floor(deduction * 0.3))),
    deduction,
  };
}

function countSuppressions(cache: FileCache): number {
  let count = 0;
  const glob = new Glob("src/**/*.{svelte,ts,js}");
  const feTags = ["fe141", "fe142", "fe143", "fe145", "fe146", "fe147", "fe148"];
  for (const file of glob.scanSync(".")) {
    if (file.includes("/tests/") || file.includes(".test.") || file.includes(".spec.")) continue;
    const content = cache.get(file);
    for (const tag of feTags) {
      const marker = `[arch:allow-${tag}]`;
      let idx = content.indexOf(marker);
      while (idx !== -1) { count++; idx = content.indexOf(marker, idx + 1); }
    }
  }
  return count;
}

function countExpiredSuppressions(cache: FileCache): number {
  let count = 0;
  const now = new Date();
  const glob = new Glob("src/**/*.{svelte,ts,js}");
  const feTags = ["fe141", "fe142", "fe143", "fe145", "fe146", "fe147", "fe148"];
  for (const file of glob.scanSync(".")) {
    if (file.includes("/tests/") || file.includes(".test.") || file.includes(".spec.")) continue;
    const content = cache.get(file);
    const lines = content.split("\n");
    for (const tag of feTags) {
      for (let i = 0; i < lines.length; i++) {
        if (lines[i].includes(`[arch:allow-${tag}]`)) {
          const dateMatch = lines[i].match(/Date\s*:\s*(\d{4}-\d{2}-\d{2})/);
          if (dateMatch) {
            const d = new Date(dateMatch[1]);
            const diffDays = (now.getTime() - d.getTime()) / (1000 * 60 * 60 * 24);
            if (diffDays > 90) count++;
          }
        }
      }
    }
  }
  return count;
}

function countStaleApprovals(): number {
  try {
    const app = readFileSync("docs/governance/frontend/GOVERNANCE_APPROVALS.md", "utf-8");
    const dates = [...app.matchAll(/\|(\d{4}-\d{2}-\d{2})\s+\|/g)];
    const now = new Date();
    let stale = 0;
    for (const [, d] of dates) {
      const date = new Date(d);
      const diffDays = (now.getTime() - date.getTime()) / (1000 * 60 * 60 * 24);
      if (diffDays > 90) stale++;
    }
    return stale;
  } catch { return 0; }
}

function countSnapshotDrift(cache: FileCache): number {
  let count = 0;
  const required = ["contracts.snapshot.json", "domain-ownership.snapshot.json",
    "projection-ownership.snapshot.json", "import-graph.snapshot.json"];
  for (const snap of required) {
    try {
      const parsed = JSON.parse(cache.get(`${SNAPSHOT_DIR}/${snap}`));
      if (!parsed.generated) count++;
    } catch { count++; }
  }
  return count;
}

function countOrphanFiles(cache: FileCache): number {
  let count = 0;
  // Orphan pages not in App.svelte
  try {
    const appContent = cache.get("src/App.svelte");
    for (const file of new Glob("src/pages/**/*.svelte").scanSync(".")) {
      const name = file.split("/").pop()?.replace(".svelte", "") || "";
      if (name === "NotFoundPage") continue;
      if (!appContent.includes(name)) count++;
    }
  } catch { /* */ }
  return count;
}

function countOwnershipExceptions(): number {
  // Count cross-domain exceptions in DOMAIN_REGISTRY
  try {
    const scannerContent = readFileSync("scripts/governance/scanner.ts", "utf-8");
    const matches = [...scannerContent.matchAll(/crossDomainExceptions:\s*\[(.*?)\]/gs)];
    let count = 0;
    for (const m of matches) {
      const items = m[1].split(",").filter((s) => s.trim().length > 0 && s.trim() !== "");
      count += items.length;
    }
    return count;
  } catch { return 0; }
}

function computeSubScore(cache: FileCache, totalDeduction: number, _patterns: string[]): number {
  return Math.max(0, Math.min(100, 100 - totalDeduction));
}

export function generateHealthScoreReport(data: HealthScoreData): string {
  const lines: string[] = [
    "# Governance Health Score",
    "",
    `Generated: ${new Date().toISOString()}`,
    "",
    "---",
    "",
    `## Health Score: ${Math.max(0, 100 - data.deduction)}/100`,
    "",
    "Informational only. Never fails build.",
    "",
    "### Breakdown",
    "",
    `| Dimension | Score |`,
    `|-----------|-------|`,
    `| Architecture Graph | ${data.architectureGraph}/100 |`,
    `| Runtime Safety | ${data.runtimeSafety}/100 |`,
    `| Contract Boundary | ${data.contractBoundary}/100 |`,
    `| Projection Integrity | ${data.projectionIntegrity}/100 |`,
    `| Governance Freeze | ${data.governanceFreeze}/100 |`,
    "",
    "### Deductions",
    "",
    `| Factor | Points |`,
    `|--------|--------|`,
    `| Active suppressions | varies (calculated) |`,
    `| Stale approvals | varies (calculated) |`,
    `| Snapshot drift | varies (calculated) |`,
    `| Orphan files | varies (calculated) |`,
    `| Ownership exceptions | varies (calculated) |`,
    `| Expired suppressions | varies (calculated) |`,
    "",
    `**Total deduction: ${data.deduction} points**`,
    "",
    "---",
    "",
    "## Trend Analysis",
    "",
    "Run regularly to track governance health over time. No enforcement.",
    "",
  ];
  return lines.join("\n");
}
