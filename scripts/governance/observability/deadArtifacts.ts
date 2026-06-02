import { Glob } from "bun";
import { readFileSync } from "fs";
import type { FileCache } from "../scanner";
import { DOMAIN_REGISTRY } from "../scanner";
import type { DeadArtifact } from "./types";

export function detectDeadArtifacts(cache: FileCache): DeadArtifact[] {
  const dead: DeadArtifact[] = [];
  const existingPages = new Set(
    [...new Glob("src/pages/**/*.svelte").scanSync(".")]
      .map((f) => f.split("/").pop()!.replace(".svelte", ""))
      .filter((n) => n !== "NotFoundPage" && n !== "App"),
  );

  const allExistingExports = new Set<string>();
  for (const file of new Glob("src/lib/contracts/*.contract.ts").scanSync(".")) {
    const content = cache.get(file);
    for (const m of content.matchAll(/export\s+async\s+function\s+(\w+)/g)) allExistingExports.add(m[1]);
  }

  // Unused page references in domain registry
  for (const [domain, reg] of Object.entries(DOMAIN_REGISTRY)) {
    for (const page of reg.pages) {
      if (!existingPages.has(page)) {
        dead.push({
          kind: "Unused page reference",
          name: page,
          location: `domain '${domain}' in DOMAIN_REGISTRY`,
          reason: `Page '${page}' registered but no such file exists`,
        });
      }
    }

    // Unused function references
    for (const fn of reg.functions) {
      if (!allExistingExports.has(fn)) {
        dead.push({
          kind: "Unused function reference",
          name: fn,
          location: `domain '${domain}' in DOMAIN_REGISTRY`,
          reason: `Function '${fn}' registered but no export exists in any contract`,
        });
      }
    }

    // Unused cross-domain exceptions
    for (const fn of reg.crossDomainExceptions) {
      if (fn && !allExistingExports.has(fn)) {
        dead.push({
          kind: "Unused cross-domain exception",
          name: fn,
          location: `domain '${domain}' in DOMAIN_REGISTRY`,
          reason: `Exception '${fn}' does not correspond to any existing contract export`,
        });
      }
    }
  }

  // Abandoned contracts (exports not called by any page)
  const allPageContent: string[] = [];
  for (const file of new Glob("src/pages/**/*.svelte").scanSync(".")) {
    if (file.includes("/tests/")) continue;
    try { allPageContent.push(cache.get(file)); } catch { /* */ }
  }
  const allPageJoined = allPageContent.join("\n");

  for (const file of new Glob("src/lib/contracts/*.contract.ts").scanSync(".")) {
    const content = cache.get(file);
    const contractName = file.split("/").pop()?.replace(".contract.ts", "") || "";
    if (contractName === "platform") continue;
    const exports = [...content.matchAll(/export\s+async\s+function\s+(\w+)/g)].map((m) => m[1]);
    const used = exports.some((fn) => new RegExp(`\\b${fn}\\b`).test(allPageJoined));
    if (!used && exports.length > 0) {
      dead.push({
        kind: "Abandoned contract",
        name: contractName,
        location: file,
        reason: "No page consumes any exported function from this contract",
      });
    }
  }

  return dead;
}

export function generateDeadArtifactsReport(artifacts: DeadArtifact[]): string {
  const lines: string[] = [
    "# Dead Artifact Detection",
    "",
    `Generated: ${new Date().toISOString()}`,
    "",
    "---",
    "",
    artifacts.length === 0
      ? "No dead artifacts detected."
      : `## Found ${artifacts.length} dead artifact(s)`,
    "",
  ];

  if (artifacts.length === 0) {
    lines.push("");
    lines.push("All governance artifacts are active and in use.");
    lines.push("");
    lines.push("---", "");
    lines.push("## Observability Note", "");
    lines.push("This report is informational only. No enforcement. No build failure.");
    lines.push("");
    return lines.join("\n");
  }

  const byKind = new Map<string, DeadArtifact[]>();
  for (const a of artifacts) {
    if (!byKind.has(a.kind)) byKind.set(a.kind, []);
    byKind.get(a.kind)!.push(a);
  }

  for (const [kind, items] of byKind) {
    lines.push(`## ${kind} (${items.length})`, "");
    for (const item of items) {
      lines.push(`- **${item.name}** — ${item.reason}`);
      lines.push(`  - Location: ${item.location}`);
    }
    lines.push("");
  }

  lines.push("---", "");
  lines.push("## Observability Note", "");
  lines.push("This report is informational only. No enforcement. No build failure.");
  lines.push("");

  return lines.join("\n");
}
