import { Glob } from "bun";
import type { Violation } from "../types";
import type { FileCache } from "../scanner";
import { DOMAIN_REGISTRY } from "../scanner";
import {
  validateSnapshotsExist,
  compareContractsSnapshot,
  compareDomainOwnershipSnapshot,
  compareProjectionSnapshot,
  validateSnapshotStructure,
  validateApprovals,
} from "../snapshot";

const INVARIANT = "ARCHITECTURE_GRAPH" as const;
const SNAPSHOT_DIR = "docs/governance/frontend/baselines";

function checkRule(
  cache: FileCache,
  violations: Violation[],
  rule: string,
  message: string,
  patterns: string[],
  regex: RegExp,
  excludeFn: (line: string, index: number, lines: string[]) => boolean,
  severity: "ERROR" | "WARNING",
  filterFn?: (file: string) => boolean,
): void {
  const scannedFiles = new Set<string>();

  for (const pattern of patterns) {
    for (const file of new Glob(pattern).scanSync(".")) {
      const normalizedFile = file.replace(/\\/g, "/");
      if (scannedFiles.has(normalizedFile)) continue;
      scannedFiles.add(normalizedFile);

      if (filterFn && !filterFn(file)) continue;

      const content = cache.get(file);
      const lines = content.split("\n");

      for (let i = 0; i < lines.length; i++) {
        const line = lines[i];
        if (line.trim().startsWith("//")) continue;
        if (regex.test(line) && !excludeFn(line, i, lines)) {
          violations.push({
            invariant: INVARIANT,
            severity,
            file,
            line: i + 1,
            message,
            rule,
          });
        }
      }
    }
  }
}

export function scanArchitectureGraph(cache: FileCache): Violation[] {
  const violations: Violation[] = [];

  // FE-131: Page must not import another page
  checkRule(
    cache,
    violations,
    "FE-131",
    "Page importing another page (pages must be independent)",
    ["src/pages/**/*.svelte"],
    /import\s+.*from\s+['"]\.\.?\/pages\//,
    (line) => {
      if (line.trim().startsWith("//") || line.trim().startsWith("<!--")) return true;
      return false;
    },
    "ERROR",
  );

  // FE-132: Component must not import a page
  checkRule(
    cache,
    violations,
    "FE-132",
    "Component importing a page (components must not depend on pages)",
    ["src/components/**/*.svelte", "src/lib/components/**/*.svelte"],
    /import\s+.*from\s+['"].*pages\//,
    (line) => {
      if (line.trim().startsWith("//") || line.trim().startsWith("<!--")) return true;
      return false;
    },
    "ERROR",
  );

  // FE-155: Architecture Drift Detection — orphan pages/contracts
  {
    const fe155Messages: string[] = [];

    try {
      const appContent = cache.get("src/App.svelte");
      for (const file of new Glob("src/pages/**/*.svelte").scanSync(".")) {
        const fileName = file.split("/").pop()?.replace(".svelte", "") || "";
        if (fileName === "NotFoundPage") continue;
        if (!appContent.includes(fileName)) {
          fe155Messages.push(`Orphan page: ${file} — not registered in App.svelte router`);
        }
      }
    } catch { /* no App.svelte */ }

    const allPageContent: string[] = [];
    for (const file of new Glob("src/pages/**/*.svelte").scanSync(".")) {
      if (file.includes("/tests/")) continue;
      allPageContent.push(cache.get(file));
    }
    const allPageContentJoined = allPageContent.join("\n");

    for (const file of new Glob("src/lib/contracts/*.contract.ts").scanSync(".")) {
      const contractName = file.replace(/^.*\/(\w+)\.contract\.ts$/, "$1");
      if (contractName === "platform") continue;
      const contractContent = cache.get(file);
      const exports = [...contractContent.matchAll(/export\s+async\s+function\s+(\w+)/g)].map((m) => m[1]);
      const used = exports.some((fn) => new RegExp(`\\b${fn}\\b`).test(allPageContentJoined));
      if (!used) {
        fe155Messages.push(`Orphan contract: ${file} — no page consumes any exported function`);
      }
    }

    for (const msg of fe155Messages) {
      violations.push({
        invariant: INVARIANT,
        severity: "WARNING",
        file: "src/",
        message: `Architecture drift detected: ${msg}`,
        rule: "FE-155",
      });
    }
  }

  // FE-156: Contract Size Governance — max 50 exports, max 500 LOC
  {
    for (const file of new Glob("src/lib/contracts/*.contract.ts").scanSync(".")) {
      const content = cache.get(file);
      const loc = content.split("\n").length;
      const exportCount = (content.match(/export\s+async\s+function\s+\w+/g) || []).length;
      if (exportCount > 50) {
        violations.push({
          invariant: INVARIANT,
          severity: "WARNING",
          file,
          message: `Has ${exportCount} exported functions (max 50)`,
          rule: "FE-156",
        });
      }
      if (loc > 500) {
        violations.push({
          invariant: INVARIANT,
          severity: "WARNING",
          file,
          message: `Has ${loc} lines (max 500)`,
          rule: "FE-156",
        });
      }
    }
  }

  // FE-158: Governance Drift Detection — compare current state against snapshots
  {
    const snapshots = [
      { file: "contracts.snapshot.json", name: "contract" },
      { file: "domain-ownership.snapshot.json", name: "domain-ownership" },
      { file: "projection-ownership.snapshot.json", name: "projection-ownership" },
      { file: "import-graph.snapshot.json", name: "import-graph" },
    ];

    violations.push(
      ...validateSnapshotsExist(cache, SNAPSHOT_DIR, snapshots.map((s) => s.file), INVARIANT),
    );
    violations.push(...compareContractsSnapshot(cache, SNAPSHOT_DIR, INVARIANT));
    violations.push(...compareDomainOwnershipSnapshot(cache, SNAPSHOT_DIR, INVARIANT));
  }

  // FE-159: Contract Mutation Detection — functions duplicated across contracts
  {
    const fnToContracts = new Map<string, string[]>();
    for (const file of new Glob("src/lib/contracts/*.contract.ts").scanSync(".")) {
      const content = cache.get(file);
      const name = file.split("/").pop()!.replace(".contract.ts", "");
      const exports = [...content.matchAll(/export\s+async\s+function\s+(\w+)/g)].map((m) => m[1]);
      for (const fn of exports) {
        if (!fnToContracts.has(fn)) fnToContracts.set(fn, []);
        fnToContracts.get(fn)!.push(name);
      }
    }
    for (const [fn, contracts] of fnToContracts) {
      if (contracts.length > 1) {
        violations.push({
          invariant: INVARIANT,
          severity: "ERROR",
          file: contracts.join(", "),
          message: `Function '${fn}' is duplicated across contracts: ${contracts.join(", ")} (contract mutation)`,
          rule: "FE-159",
        });
      }
    }
  }

  // FE-160: Projection Mutation Detection
  {
    violations.push(...compareProjectionSnapshot(cache, SNAPSHOT_DIR, INVARIANT));
  }

  // FE-163: Dead Governance Artifact Detection
  {
    const existingPages = new Set(
      [...new Glob("src/pages/**/*.svelte").scanSync(".")]
        .map((f) => f.split("/").pop()!.replace(".svelte", ""))
        .filter((n) => n !== "NotFoundPage" && n !== "App"),
    );

    for (const [domain, reg] of Object.entries(DOMAIN_REGISTRY)) {
      for (const page of reg.pages) {
        if (!existingPages.has(page)) {
          violations.push({
            invariant: INVARIANT,
            severity: "WARNING",
            file: `src/pages/`,
            message: `Dead governance entry — page '${page}' registered for domain '${domain}' but no such page exists`,
            rule: "FE-163",
          });
        }
      }
    }

    const allExistingExports = new Set<string>();
    for (const file of new Glob("src/lib/contracts/*.contract.ts").scanSync(".")) {
      const content = cache.get(file);
      for (const m of content.matchAll(/export\s+async\s+function\s+(\w+)/g)) allExistingExports.add(m[1]);
    }

    for (const [domain, reg] of Object.entries(DOMAIN_REGISTRY)) {
      for (const fn of reg.functions) {
        if (!allExistingExports.has(fn)) {
          violations.push({
            invariant: INVARIANT,
            severity: "WARNING",
            file: `src/lib/contracts/`,
            message: `Dead governance entry — function '${fn}' registered for domain '${domain}' but no such export exists`,
            rule: "FE-163",
          });
        }
      }
      for (const fn of reg.crossDomainExceptions) {
        if (!allExistingExports.has(fn) && fn !== "") {
          violations.push({
            invariant: INVARIANT,
            severity: "WARNING",
            file: `src/lib/contracts/`,
            message: `Unused cross-domain exception — '${fn}' in domain '${domain}' does not correspond to any existing export`,
            rule: "FE-163",
          });
        }
      }
    }
  }

  return violations;
}
