import { Glob } from "bun";
import { readFileSync } from "fs";
import type { FileCache } from "../scanner";
import { DOMAIN_REGISTRY } from "../scanner";
import type { SelfAuditItem } from "./types";

export function runSelfAudit(cache: FileCache): SelfAuditItem[] {
  const items: SelfAuditItem[] = [];

  // Active suppressions with risk
  const feTags = ["fe141", "fe142", "fe143", "fe145", "fe146", "fe147", "fe148"];
  const now = new Date();
  const glob = new Glob("src/**/*.{svelte,ts,js}");
  for (const file of glob.scanSync(".")) {
    if (file.includes("/tests/") || file.includes(".test.") || file.includes(".spec.")) continue;
    const content = cache.get(file);
    const lines = content.split("\n");
    for (let i = 0; i < lines.length; i++) {
      for (const tag of feTags) {
        const marker = `[arch:allow-${tag}]`;
        if (lines[i].includes(marker)) {
          const dateMatch = lines[i].match(/Date\s*:\s*(\d{4}-\d{2}-\d{2})/);
          let risk: "LOW" | "MEDIUM" | "HIGH" = "LOW";
          if (dateMatch) {
            const d = new Date(dateMatch[1]);
            const diffDays = (now.getTime() - d.getTime()) / (1000 * 60 * 60 * 24);
            if (diffDays > 90) risk = "HIGH";
            else if (diffDays > 60) risk = "MEDIUM";
          }
          items.push({
            type: "Suppression",
            file: `${file}:${i + 1}`,
            detail: `Tag: ${marker}`,
            risk,
          });
        }
      }
    }
  }

  // Approvals inventory
  try {
    const appContent = readFileSync("docs/governance/frontend/GOVERNANCE_APPROVALS.md", "utf-8");
    const approvals = [...appContent.matchAll(/\|\s*(\d{4}-\d{2}-\d{2})\s+\|(.+?)\|/g)];
    for (const m of approvals) {
      items.push({
        type: "Approval",
        file: "GOVERNANCE_APPROVALS.md",
        detail: m[2].trim(),
        risk: "LOW",
      });
    }
  } catch { /* */ }

  // Snapshots inventory
  for (const file of new Glob("docs/governance/frontend/baselines/*.snapshot.json").scanSync(".")) {
    try {
      const parsed = JSON.parse(readFileSync(file, "utf-8"));
      const generated = parsed.generated || "unknown";
      items.push({
        type: "Snapshot",
        file,
        detail: `Generated: ${generated}`,
        risk: "LOW",
      });
    } catch {
      items.push({ type: "Snapshot", file, detail: "Invalid or unparseable", risk: "HIGH" });
    }
  }

  // Ownership exceptions inventory
  for (const [domain, reg] of Object.entries(DOMAIN_REGISTRY)) {
    if (reg.crossDomainExceptions.length > 0) {
      items.push({
        type: "Ownership Exception",
        file: `scripts/governance/scanner.ts (domain: ${domain})`,
        detail: `Cross-domain exceptions: ${reg.crossDomainExceptions.join(", ")}`,
        risk: "MEDIUM",
      });
    }
  }

  // Contract inventory
  for (const file of new Glob("src/lib/contracts/*.contract.ts").scanSync(".")) {
    try {
      const content = cache.get(file);
      const exports = [...content.matchAll(/export\s+async\s+function\s+(\w+)/g)];
      items.push({
        type: "Contract",
        file,
        detail: `${exports.length} exports`,
        risk: "LOW",
      });
    } catch {
      items.push({ type: "Contract", file, detail: "Cannot read", risk: "MEDIUM" });
    }
  }

  return items;
}

export function generateSelfAuditReport(items: SelfAuditItem[]): string {
  const lowRisk = items.filter((i) => i.risk === "LOW");
  const medRisk = items.filter((i) => i.risk === "MEDIUM");
  const highRisk = items.filter((i) => i.risk === "HIGH");

  const lines: string[] = [
    "# Governance Self-Audit Report",
    "",
    `Generated: ${new Date().toISOString()}`,
    "",
    "---",
    "",
    "## Risk Summary",
    "",
    `| Risk Level | Count |`,
    `|------------|-------|`,
    `| LOW RISK | ${lowRisk.length} |`,
    `| MEDIUM RISK | ${medRisk.length} |`,
    `| HIGH RISK | ${highRisk.length} |`,
    "",
    "---",
    "",
  ];

  if (highRisk.length > 0) {
    lines.push("## HIGH RISK Items", "");
    for (const item of highRisk) {
      lines.push(`- **${item.type}**: ${item.file} — ${item.detail}`);
    }
    lines.push("", "---", "");
  }

  if (medRisk.length > 0) {
    lines.push("## MEDIUM RISK Items", "");
    for (const item of medRisk) {
      lines.push(`- **${item.type}**: ${item.file} — ${item.detail}`);
    }
    lines.push("", "---", "");
  }

  if (lowRisk.length > 0) {
    lines.push("## LOW RISK Items", "");
    for (const item of lowRisk) {
      lines.push(`- **${item.type}**: ${item.file} — ${item.detail}`);
    }
    lines.push("", "---", "");
  }

  lines.push("", "## Observability Note", "");
  lines.push("This report is informational only. No enforcement. No build failure.");
  lines.push("");

  return lines.join("\n");
}
