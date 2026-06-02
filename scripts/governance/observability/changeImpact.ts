import { execSync } from "child_process";

interface ChangeImpact {
  file: string;
  impactedInvariants: string[];
  impactedContracts: string[];
  impactArea: string[];
  risk: "LOW" | "MEDIUM" | "HIGH";
}

// Map file patterns to affected invariants
const INVARIANT_MAP: Array<{ pattern: RegExp; invariant: string }> = [
  { pattern: /src\/lib\/contracts\//, invariant: "Contract Boundary" },
  { pattern: /src\/pages\//, invariant: "Projection Integrity" },
  { pattern: /src\/components\//, invariant: "Projection Integrity" },
  { pattern: /src\/lib\/types\.ts/, invariant: "Projection Integrity" },
  { pattern: /src\/lib\/.*\.ts/, invariant: "Runtime Safety" },
  { pattern: /src\/lib\/session\.ts/, invariant: "Runtime Safety" },
  { pattern: /scripts\/governance\//, invariant: "All invariants" },
  { pattern: /docs\/governance/, invariant: "Governance Freeze" },
  { pattern: /\.snapshot\.json$/, invariant: "Governance Freeze" },
  { pattern: /GOVERNANCE_/, invariant: "Governance Freeze" },
  { pattern: /baselines\//, invariant: "Architecture Graph" },
  { pattern: /src\/App\.svelte/, invariant: "Architecture Graph" },
];

// Map files to contracts
const CONTRACT_PATTERN = /src\/lib\/contracts\/(\w+)\.contract\.ts/;

// Map files to ownership areas
const OWNERSHIP_MAP: Array<{ pattern: RegExp; area: string }> = [
  { pattern: /src\/pages\/(\w+)/, area: "Pages" },
  { pattern: /src\/components\//, area: "Components" },
  { pattern: /src\/lib\/contracts\//, area: "Contracts" },
  { pattern: /scripts\/governance\//, area: "Governance Engine" },
  { pattern: /docs\/governance/, area: "Governance Documentation" },
  { pattern: /src\/lib\//, area: "Shared Library" },
];

export function detectChangedFiles(): string[] {
  try {
    const output = execSync("git diff --name-only HEAD~1", { encoding: "utf-8", timeout: 5000 });
    return output.trim().split("\n").filter(Boolean);
  } catch {
    // Fallback: use git status
    try {
      const output = execSync("git status --porcelain", { encoding: "utf-8", timeout: 5000 });
      return output.trim().split("\n")
        .filter(Boolean)
        .map((line) => line.substring(3).trim());
    } catch {
      return [];
    }
  }
}

export function analyzeChangeImpact(): ChangeImpact[] {
  const changedFiles = detectChangedFiles();
  if (changedFiles.length === 0) return [];

  const impacts: ChangeImpact[] = [];

  for (const file of changedFiles) {
    if (!file || file.includes("node_modules") || file.startsWith(".")) continue;

    const impactedInvariants = new Set<string>();
    for (const { pattern, invariant } of INVARIANT_MAP) {
      if (pattern.test(file)) impactedInvariants.add(invariant);
    }

    const impactedContracts = new Set<string>();
    const contractMatch = file.match(CONTRACT_PATTERN);
    if (contractMatch) impactedContracts.add(contractMatch[1]);

    const impactArea = new Set<string>();
    for (const { pattern, area } of OWNERSHIP_MAP) {
      if (pattern.test(file)) impactArea.add(area);
    }

    let risk: "LOW" | "MEDIUM" | "HIGH" = "LOW";
    if (impactedInvariants.size >= 3) risk = "HIGH";
    else if (impactedInvariants.size >= 2) risk = "MEDIUM";
    else if (impactedInvariants.has("All invariants")) risk = "HIGH";

    // Also check for governance engine changes
    if (file.startsWith("scripts/governance/") || file.startsWith("check_arch.ts")) risk = "HIGH";

    impacts.push({
      file,
      impactedInvariants: [...impactedInvariants],
      impactedContracts: [...impactedContracts],
      impactArea: [...impactArea],
      risk,
    });
  }

  return impacts;
}

export function generateChangeImpactReport(impacts: ChangeImpact[]): string {
  if (impacts.length === 0) {
    return [
      "# Change Impact Analysis",
      "",
      `Generated: ${new Date().toISOString()}`,
      "",
      "---",
      "",
      "No changed files detected since last commit.",
      "",
    ].join("\n");
  }

  const lines: string[] = [
    "# Change Impact Analysis",
    "",
    `Generated: ${new Date().toISOString()}`,
    "",
    "---",
    "",
    `## Overview`,
    "",
    `Changed files detected: ${impacts.length}`,
    "",
    "---",
    "",
    "## Impact Details",
    "",
  ];

  for (const impact of impacts) {
    const riskIcon = impact.risk === "HIGH" ? "🔴" : impact.risk === "MEDIUM" ? "🟡" : "🟢";
    lines.push(`### Changed: ${impact.file}`, "");
    lines.push(`**Risk:** ${riskIcon} ${impact.risk}`);
    if (impact.impactedInvariants.length > 0) {
      lines.push(`**Impacted Invariants:** ${impact.impactedInvariants.join(", ")}`);
    }
    if (impact.impactedContracts.length > 0) {
      lines.push(`**Impacted Contracts:** ${impact.impactedContracts.join(", ")}`);
    }
    if (impact.impactArea.length > 0) {
      lines.push(`**Ownership Areas:** ${impact.impactArea.join(", ")}`);
    }
    lines.push("");
  }

  lines.push("---", "");
  lines.push("## Observability Note", "");
  lines.push("This report is informational only. No enforcement. No build failure.");
  lines.push("");

  return lines.join("\n");
}
