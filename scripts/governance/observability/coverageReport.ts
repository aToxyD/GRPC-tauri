import { Glob } from "bun";
import { readFileSync } from "fs";
import type { FileCache } from "../scanner";
import type { CoverageReportData } from "./types";

const SNAPSHOT_DIR = "docs/governance/frontend/baselines";

interface CoverageBucket {
  label: string;
  covered: number;
  total: number;
}

const pct = (covered: number, total: number): string =>
  total === 0 ? "—" : `${Math.round((covered / total) * 100)}%`;

function countUnique(references: string[], candidates: string[]): number {
  const refs = new Set(references);
  return candidates.filter((c) => refs.has(c)).length;
}

function collectNames(globs: string[], exclude: RegExp): string[] {
  const names: string[] = [];
  for (const pattern of globs) {
    for (const file of new Glob(pattern).scanSync(".")) {
      if (exclude.test(file)) continue;
      const name = file.split("/").pop()?.replace(/\.(svelte|ts)$/, "") || "";
      if (name) names.push(name);
    }
  }
  return names;
}

export function computeCoverageReport(cache: FileCache): CoverageReportData {
  // 1. IPC isolation: invoke / @tauri-apps confined to src/lib/tauri.ts.
  //    src/main.ts is a documented exception (window.__TAURI__ bridge for the
  //    router bootstrap), matching check_arch rules 27/27b exclusions.
  const ipcSources = [...new Glob("src/**/*.{ts,svelte}").scanSync(".")]
    .filter((f) => !f.includes("/tests/") && !f.includes(".test.") && !f.includes(".spec."));
  let ipcLeaks = 0;
  for (const file of ipcSources) {
    if (file === "src/lib/tauri.ts") continue;
    if (file === "src/main.ts") continue;
    const content = cache.get(file);
    if (/@tauri-apps\/|invoke\s*\(/.test(content)) ipcLeaks++;
  }

  // 2. Contract coverage: contracts referenced from pages/components
  const contractFiles = [...new Glob("src/lib/contracts/*.contract.ts").scanSync(".")]
    .filter((f) => !f.includes("/tests/"));
  const contractNames = contractFiles.map((f) => f.split("/").pop()?.replace(/\.ts$/, "") || "");
  const consumerSources = [
    ...new Glob("src/pages/**/*.svelte").scanSync("."),
    ...new Glob("src/components/**/*.svelte").scanSync("."),
    ...new Glob("src/lib/**/*.ts").scanSync("."),
  ].filter((f) => !f.includes("/tests/"));
  const contractRefs: string[] = [];
  for (const file of consumerSources) {
    const content = cache.get(file);
    for (const name of contractNames) {
      if (content.includes(name)) contractRefs.push(name);
    }
  }

  // 3. Projection coverage: pages that consume projections via the IPC
  //    contract layer (src/lib/tauri.ts) rather than importing contracts
  //    directly (direct contract imports are prohibited).
  const pageFiles = [...new Glob("src/pages/**/*.svelte").scanSync(".")]
    .filter((f) => !f.includes("/tests/") && !f.includes("/e2e/"));
  const pagesWithProjections = pageFiles.filter((f) => {
    const content = cache.get(f);
    return /lib\/tauri/.test(content);
  });

  // 4. Page registration coverage: pages referenced from App.svelte.
  //    NotFoundPage is a routing fallback, never statically registered.
  const pageNames = pageFiles
    .map((f) => f.split("/").pop()?.replace(/\.svelte$/, "") || "")
    .filter((name) => name !== "NotFoundPage");
  let appRefs: string[] = [];
  try {
    const appContent = cache.get("src/App.svelte");
    appRefs = pageNames.filter((name) => appContent.includes(name));
  } catch { /* App.svelte absent */ }

  // 5. Component coverage: components referenced anywhere in src
  const componentNames = [
    ...collectNames(["src/components/**/*.svelte"], /tests/),
    ...collectNames(["src/lib/components/**/*.svelte"], /tests/),
  ];
  const componentRefs: string[] = [];
  for (const file of consumerSources) {
    const content = cache.get(file);
    for (const name of componentNames) {
      if (content.includes(name)) componentRefs.push(name);
    }
  }

  // 6. Snapshot coverage: required baselines present and valid
  const requiredSnapshots = [
    "contracts.snapshot.json",
    "domain-ownership.snapshot.json",
    "projection-ownership.snapshot.json",
    "import-graph.snapshot.json",
  ];
  const validSnapshots = requiredSnapshots.filter((snap) => {
    try {
      const parsed = JSON.parse(cache.get(`${SNAPSHOT_DIR}/${snap}`));
      return Boolean(parsed.generated);
    } catch { return false; }
  });

  // 7. Suppression coverage: suppressions tagged with an expiry date
  const suppressionsTotal: string[] = [];
  const suppressionsDated: string[] = [];
  const suppressionGlob = new Glob("src/**/*.{svelte,ts,js}");
  const feTags = ["fe141", "fe142", "fe143", "fe145", "fe146", "fe147", "fe148"];
  for (const file of suppressionGlob.scanSync(".")) {
    if (file.includes("/tests/") || file.includes(".test.") || file.includes(".spec.")) continue;
    const content = cache.get(file);
    for (const tag of feTags) {
      const marker = `[arch:allow-${tag}]`;
      let idx = content.indexOf(marker);
      while (idx !== -1) {
        suppressionsTotal.push(marker);
        const lineStart = content.lastIndexOf("\n", idx) + 1;
        const lineEnd = content.indexOf("\n", idx);
        const line = content.substring(lineStart, lineEnd >= 0 ? lineEnd : undefined);
        if (/Date\s*:\s*\d{4}-\d{2}-\d{2}/.test(line)) suppressionsDated.push(marker);
        idx = content.indexOf(marker, idx + 1);
      }
    }
  }

  const buckets: CoverageBucket[] = [
    {
      label: "IPC isolation (invoke/@tauri-apps confined to src/lib/tauri.ts)",
      covered: ipcSources.length - ipcLeaks,
      total: ipcSources.length,
    },
    {
      label: "Contract consumers (pages/components/lib referencing contracts)",
      covered: countUnique(contractRefs, contractNames),
      total: contractNames.length,
    },
    {
      label: "Pages consuming projections via IPC contract layer",
      covered: pagesWithProjections.length,
      total: pageFiles.length,
    },
    {
      label: "Pages registered in src/App.svelte",
      covered: appRefs.length,
      total: pageNames.length,
    },
    {
      label: "Components referenced from src",
      covered: countUnique(componentRefs, componentNames),
      total: componentNames.length,
    },
    {
      label: "Required governance snapshots present and valid",
      covered: validSnapshots.length,
      total: requiredSnapshots.length,
    },
    {
      label: "Suppressions tagged with expiry date",
      covered: suppressionsDated.length,
      total: suppressionsTotal.length,
    },
  ];

  return {
    ipcIsolationViolations: ipcLeaks,
    buckets,
    overallCoverage: Math.round(
      buckets.reduce((acc, b) => acc + (b.total > 0 ? b.covered / b.total : 1), 0) /
        buckets.length *
        100,
    ),
  };
}

export function generateCoverageReport(data: CoverageReportData): string {
  const lines: string[] = [
    "# Frontend Governance Coverage Report",
    "",
    `Generated: ${new Date().toISOString()}`,
    "",
    "---",
    "",
    "## Coverage Metrics",
    "",
    "Coverage measures how completely the frontend complies with and is",
    "instrumented against the documented governance invariants.",
    "",
    `**Overall coverage: ${data.overallCoverage}%** (informational estimate)`,
    "",
    "| Dimension | Covered | Total | Coverage |",
    "|-----------|---------|-------|----------|",
    ...data.buckets.map(
      (b) => `| ${b.label} | ${b.covered} | ${b.total} | ${pct(b.covered, b.total)} |`,
    ),
    "",
    "---",
    "",
    "## IPC Isolation",
    "",
    `Violations found outside src/lib/tauri.ts: **${data.ipcIsolationViolations}**`,
    "",
    "## Observability Note",
    "",
    "Informational only. Never fails build. All metrics are computed from current source.",
    "",
  ];
  return lines.join("\n");
}
