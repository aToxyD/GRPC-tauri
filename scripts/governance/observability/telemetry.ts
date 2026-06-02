import { Glob } from "bun";
import { readFileSync, writeFileSync, existsSync, mkdirSync } from "fs";
import { dirname } from "path";
import type { FileCache } from "../scanner";
import type { TelemetryData } from "./types";

const SNAPSHOT_DIR = "docs/governance/frontend/baselines";
const TELEMETRY_STORE = "docs/governance/frontend/.governance-telemetry.json";

interface PreviousTelemetry {
  generated: string;
  invariantCount: number;
  snapshotCount: number;
  approvalCount: number;
  suppressionCount: number;
  expiredSuppressionCount: number;
  contractCount: number;
  pageCount: number;
  componentCount: number;
  ruleCount: number;
}

function loadPreviousTelemetry(): PreviousTelemetry | null {
  try {
    return JSON.parse(readFileSync(TELEMETRY_STORE, "utf-8"));
  } catch { return null; }
}

export function saveTelemetry(data: TelemetryData): void {
  try {
    const dir = dirname(TELEMETRY_STORE);
    if (!existsSync(dir)) mkdirSync(dir, { recursive: true });
    writeFileSync(TELEMETRY_STORE, JSON.stringify({ generated: new Date().toISOString(), ...data }, null, 2));
  } catch { /* best-effort */ }
}

export function computeTelemetry(cache: FileCache): TelemetryData {
  const contractCount = [...new Glob("src/lib/contracts/*.contract.ts").scanSync(".")].length;
  const pageCount = [...new Glob("src/pages/**/*.svelte").scanSync(".")]
    .filter((f) => !f.includes("/tests/") && !f.includes("/e2e/")).length;
  const componentCount = [
    ...new Glob("src/components/**/*.svelte").scanSync("."),
    ...new Glob("src/lib/components/**/*.svelte").scanSync("."),
  ].filter((f) => !f.includes("/tests/")).length;

  const snapshotCount = [...new Glob(`${SNAPSHOT_DIR}/*.snapshot.json`).scanSync(".")].length;

  let approvalCount = 0;
  try {
    const app = readFileSync("docs/governance/frontend/GOVERNANCE_APPROVALS.md", "utf-8");
    approvalCount = (app.match(/\|\s*\d{4}-\d{2}-\d{2}\s+\|/g) || []).length;
  } catch { /* ignore */ }

  let suppressionCount = 0;
  let expiredSuppressionCount = 0;
  const now = new Date();
  const glob = new Glob("src/**/*.{svelte,ts,js}");
  const feTags = ["fe141", "fe142", "fe143", "fe145", "fe146", "fe147", "fe148"];
  for (const file of glob.scanSync(".")) {
    if (file.includes("/tests/") || file.includes(".test.") || file.includes(".spec.")) continue;
    const content = cache.get(file);
    for (const tag of feTags) {
      const marker = `[arch:allow-${tag}]`;
      let idx = content.indexOf(marker);
      while (idx !== -1) {
        suppressionCount++;
        // Check expiry
        const lineStart = content.lastIndexOf("\n", idx) + 1;
        const lineEnd = content.indexOf("\n", idx);
        const line = content.substring(lineStart, lineEnd >= 0 ? lineEnd : undefined);
        const dateMatch = line.match(/Date\s*:\s*(\d{4}-\d{2}-\d{2})/);
        if (dateMatch) {
          const d = new Date(dateMatch[1]);
          const diffDays = (now.getTime() - d.getTime()) / (1000 * 60 * 60 * 24);
          if (diffDays > 90) expiredSuppressionCount++;
        }
        idx = content.indexOf(marker, idx + 1);
      }
    }
  }

  let ruleCount = 0;
  try {
    const engine = readFileSync("scripts/governance/engine.ts", "utf-8");
    ruleCount = [...engine.matchAll(/\bcheckRule\s*\(/g)].length;
  } catch { /* */ }

  const data: TelemetryData = {
    invariantCount: 5,
    snapshotCount,
    approvalCount,
    suppressionCount,
    expiredSuppressionCount,
    contractCount,
    pageCount,
    componentCount,
    ruleCount,
  };

  return data;
}

export function generateTelemetryReport(data: TelemetryData): string {
  const prev = loadPreviousTelemetry();

  const delta = (current: number, previous?: number): string => {
    if (previous === undefined || previous === null) return "—";
    const d = current - previous;
    if (d === 0) return "0";
    return d > 0 ? `+${d}` : `${d}`;
  };

  const lines: string[] = [
    "# Governance Telemetry Report",
    "",
    `Generated: ${new Date().toISOString()}`,
    "",
    "---",
    "",
    "## Governance State Summary",
    "",
    "| Metric | Current | Previous | Delta |",
    "|--------|---------|---------|-------|",
    `| Invariant count | ${data.invariantCount} | ${prev?.invariantCount ?? "—"} | ${delta(data.invariantCount, prev?.invariantCount)} |`,
    `| Snapshot count | ${data.snapshotCount} | ${prev?.snapshotCount ?? "—"} | ${delta(data.snapshotCount, prev?.snapshotCount)} |`,
    `| Approval count | ${data.approvalCount} | ${prev?.approvalCount ?? "—"} | ${delta(data.approvalCount, prev?.approvalCount)} |`,
    `| Suppression count | ${data.suppressionCount} | ${prev?.suppressionCount ?? "—"} | ${delta(data.suppressionCount, prev?.suppressionCount)} |`,
    `| Expired suppressions | ${data.expiredSuppressionCount} | ${prev?.expiredSuppressionCount ?? "—"} | ${delta(data.expiredSuppressionCount, prev?.expiredSuppressionCount)} |`,
    `| Contract count | ${data.contractCount} | ${prev?.contractCount ?? "—"} | ${delta(data.contractCount, prev?.contractCount)} |`,
    `| Page count | ${data.pageCount} | ${prev?.pageCount ?? "—"} | ${delta(data.pageCount, prev?.pageCount)} |`,
    `| Component count | ${data.componentCount} | ${prev?.componentCount ?? "—"} | ${delta(data.componentCount, prev?.componentCount)} |`,
    `| Rule count | ${data.ruleCount} | ${prev?.ruleCount ?? "—"} | ${delta(data.ruleCount, prev?.ruleCount)} |`,
    "",
    "---",
    "",
    "## Trend",
    "",
    "Current state is informational. No action required.",
    "",
  ];

  return lines.join("\n");
}
