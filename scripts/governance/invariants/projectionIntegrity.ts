import { Glob } from "bun";
import type { Violation } from "../types";
import type { FileCache } from "../scanner";
import { DOMAIN_REGISTRY, UNIVERSAL_ALLOWED } from "../scanner";
import { suppressExclude } from "../suppression";

const INVARIANT = "PROJECTION_INTEGRITY" as const;

function fileFilter(f: string): boolean {
  return !f.includes("/tests/") && !f.includes(".test.") && !f.includes(".spec.");
}

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

export function scanProjectionIntegrity(cache: FileCache): Violation[] {
  const violations: Violation[] = [];
  const patterns = ["src/pages/**/*.svelte", "src/components/**/*.svelte", "src/lib/**/*.ts"];

  // FE-141: Division on projection values
  checkRule(
    cache,
    violations,
    "FE-141",
    "Division on projection values detected — suppress with [arch:allow-fe141]",
    patterns,
    /\/(?!\d)(?=[^;]*\b(cost|average|beneficiar|quantity|total|price)\b)/i,
    suppressExclude("fe141"),
    "ERROR",
    fileFilter,
  );

  // FE-142: Frontend must not compute line totals
  checkRule(
    cache,
    violations,
    "FE-142",
    "Multiplication on projection values detected — suppress with [arch:allow-fe142]",
    patterns,
    /\*(?=[^;]*\b(unit_cost|unitCost|price|cost|quantity|amount|beneficiaries)\b)/i,
    suppressExclude("fe142"),
    "ERROR",
    fileFilter,
  );

  // FE-143: Frontend must not reconstruct aggregate costs
  checkRule(
    cache,
    violations,
    "FE-143",
    "Addition on cost values detected — suppress with [arch:allow-fe143]",
    patterns,
    /\+(?=[^;]*\b(cost|total_cost|predicted_fifo_cost)\b)/i,
    suppressExclude("fe143"),
    "ERROR",
    fileFilter,
  );

  // FE-145: Frontend must not compute consumption arithmetic
  checkRule(
    cache,
    violations,
    "FE-145",
    "Consumption arithmetic detected — suppress with [arch:allow-fe145]",
    patterns,
    /[\+\-\*\/](?=[^;]*\b(consumed|planned|mealCount|meal_count|portion|remaining|portionCount)\b)/i,
    suppressExclude("fe145"),
    "ERROR",
    fileFilter,
  );

  // FE-146: Frontend must not recompute averages
  checkRule(
    cache,
    violations,
    "FE-146",
    "Average recomputation detected — suppress with [arch:allow-fe146]",
    patterns,
    /\b(average|mealAverage|dailyAverage|avg)\s*(?=[:=])|(?<=\/)\s*\b(beneficiaries|count|days)\b/,
    suppressExclude("fe146"),
    "ERROR",
    fileFilter,
  );

  // FE-147: Frontend must not reassemble projections
  checkRule(
    cache,
    violations,
    "FE-147",
    "Projection reassembly detected — suppress with [arch:allow-fe147]",
    patterns,
    /(\.\.\.\w*project|\.\.\.\w*forecast|\.\.\.\w*report|Object\.assign\([^)]*project)/i,
    suppressExclude("fe147"),
    "ERROR",
    fileFilter,
  );

  // FE-148: Detect long $derived computation chains
  {
    const fe148tag = "[arch:allow-fe148]";
    for (const pattern of ["src/pages/**/*.svelte", "src/components/**/*.svelte"]) {
      const scannedFiles = new Set<string>();
      for (const file of new Glob(pattern).scanSync(".")) {
        const normalizedFile = file.replace(/\\/g, "/");
        if (scannedFiles.has(normalizedFile)) continue;
        scannedFiles.add(normalizedFile);
        if (file.includes("/tests/") || file.includes(".test.") || file.includes(".spec.")) continue;

        const content = cache.get(file);
        const lines = content.split("\n");
        for (let i = 0; i < lines.length; i++) {
          const line = lines[i];
          if (line.trim().startsWith("//") || line.trim().startsWith("<!--")) continue;
          if (line.includes(fe148tag)) continue;
          if (i > 0 && lines[i - 1].includes(fe148tag)) continue;
          const derivedCount = (line.match(/\$derived(?:\.\w+)?\s*\(/g) || []).length;
          if (derivedCount >= 2) {
            violations.push({
              invariant: INVARIANT,
              severity: "ERROR",
              file,
              line: i + 1,
              message: "Long $derived computation chain detected — suppress with [arch:allow-fe148]",
              rule: "FE-148",
            });
          }
        }
      }
    }
  }

  // FE-152: Projection Ownership Enforcement
  {
    const allDomainFns = new Set<string>();
    const fnToDomain = new Map<string, string>();
    for (const [domain, reg] of Object.entries(DOMAIN_REGISTRY)) {
      for (const fn of reg.functions) { allDomainFns.add(fn); fnToDomain.set(fn, domain); }
      for (const fn of reg.crossDomainExceptions) { allDomainFns.add(fn); }
    }

    const pageToDomain = new Map<string, string[]>();
    for (const [domain, reg] of Object.entries(DOMAIN_REGISTRY)) {
      for (const p of reg.pages) {
        const existing = pageToDomain.get(p) || [];
        existing.push(domain);
        pageToDomain.set(p, existing);
      }
    }

    for (const file of new Glob("src/pages/**/*.svelte").scanSync(".")) {
      const normalizedFile = file.replace(/\\/g, "/");
      if (normalizedFile.includes("/tests/") || normalizedFile.includes("/e2e/")) continue;
      const fileName = normalizedFile.split("/").pop()?.replace(".svelte", "") || "";
      if (fileName === "NotFoundPage" || fileName === "App") continue;

      const content = cache.get(file);
      const importMatch = content.match(/from\s+['"]\.\.?\/lib\/contracts['"]/);
      if (!importMatch) continue;

      const pageFileMatches: string[] = [];
      for (const fn of allDomainFns) {
        if (new RegExp(`\\b${fn}\\b`).test(content)) pageFileMatches.push(fn);
      }

      const pageDomains = pageToDomain.get(fileName) || [];
      const errors: string[] = [];

      for (const fn of pageFileMatches) {
        if (UNIVERSAL_ALLOWED.includes(fn)) continue;
        const domain = fnToDomain.get(fn);
        if (!domain) continue;
        let isAllowed = pageDomains.includes(domain);
        if (!isAllowed) {
          for (const pd of pageDomains) {
            if ((DOMAIN_REGISTRY[pd]?.crossDomainExceptions || []).includes(fn)) { isAllowed = true; break; }
          }
        }
        if (!isAllowed) errors.push(`${fn} (owned by ${domain})`);
      }

      if (errors.length > 0) {
        violations.push({
          invariant: INVARIANT,
          severity: "ERROR",
          file,
          message: `Imports projection functions from non-owned domain: ${errors.join(", ")}`,
          rule: "FE-152",
        });
      }
    }
  }

  // FE-157: Projection Surface Governance — no computed/derived/helper fields
  {
    const typeFiles = [
      "src/lib/types.ts",
      ...Array.from(new Glob("src/lib/contracts/*.contract.ts").scanSync(".")),
    ];
    const FE157_ALLOWLIST = new Set(["computed_closing", "computed_at", "reported_closing"]);

    for (const file of typeFiles) {
      const content = cache.get(file);
      const lines = content.split("\n");
      for (let i = 0; i < lines.length; i++) {
        const line = lines[i];
        const fieldMatch = line.match(/^\s+(\w[\w]*)\s*[?]?\s*:\s*.*;/);
        if (fieldMatch) {
          const fieldName = fieldMatch[1];
          if (FE157_ALLOWLIST.has(fieldName)) continue;
          const fieldLower = fieldName.toLowerCase();
          if (["computed", "derived", "helper"].includes(fieldLower) ||
            fieldLower.startsWith("computed_") || fieldLower.startsWith("derived_") || fieldLower.startsWith("helper_")) {
            violations.push({
              invariant: INVARIANT,
              severity: "ERROR",
              file,
              line: i + 1,
              message: `Field '${fieldName}' suggests frontend-convenience computation`,
              rule: "FE-157",
            });
          }
        }
        const calcMatch = line.match(/^\s+(\w*[Cc]alculated\w*)\s*[?]?\s*:/);
        if (calcMatch) {
          violations.push({
            invariant: INVARIANT,
            severity: "ERROR",
            file,
            line: i + 1,
            message: `Field '${calcMatch[1]}' suggests frontend-convenience calculation`,
            rule: "FE-157",
          });
        }
      }
    }

    try {
      const content = cache.get("src/components/consumption/types.ts");
      const lines = content.split("\n");
      for (let i = 0; i < lines.length; i++) {
        const line = lines[i];
        const fieldMatch = line.match(/^\s+(\w[\w]*)\s*[?]?\s*:\s*.*;/);
        if (fieldMatch) {
          const fieldName = fieldMatch[1].toLowerCase();
          if (["computed", "derived", "helper"].includes(fieldName) ||
            fieldName.startsWith("computed_") || fieldName.startsWith("derived_") || fieldName.startsWith("helper_")) {
            violations.push({
              invariant: INVARIANT,
              severity: "ERROR",
              file: "src/components/consumption/types.ts",
              line: i + 1,
              message: `Field '${fieldMatch[1]}' suggests frontend-convenience computation`,
              rule: "FE-157",
            });
          }
        }
      }
    } catch { /* skip */ }
  }

  return violations;
}
