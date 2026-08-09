import { Glob } from "bun";
import type { Violation } from "../types";
import type { FileCache } from "../scanner";
import { collectSuppressions, validateSuppressionMetadata } from "../suppression";

const INVARIANT = "RUNTIME_SAFETY" as const;

function fileFilter(f: string): boolean {
  return !f.includes("/tests/") && !f.includes(".test.") && !f.includes(".spec.") && !f.includes("/e2e/");
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

export function scanRuntimeSafety(cache: FileCache): Violation[] {
  const violations: Violation[] = [];
  const srcPatterns = ["src/pages/**/*.svelte", "src/components/**/*.svelte", "src/lib/components/**/*.svelte", "src/lib/**/*.ts"];

  // FE-100: Every $state() must have a // @category marker
  checkRule(
    cache,
    violations,
    "FE-100",
    "$state() declaration without @category marker",
    srcPatterns,
    /\$state\(/,
    (line, index, lines) => {
      if (line.includes("@category")) return true;
      for (let i = index - 1; i >= Math.max(0, index - 3); i--) {
        const trimmed = lines[i].trim();
        if (trimmed === "") continue;
        if (trimmed.startsWith("//") && trimmed.includes("@category")) return true;
        if (trimmed.startsWith("//")) continue;
        break;
      }
      return false;
    },
    "ERROR",
    fileFilter,
  );

  // FE-100B: Every $derived() / $derived.by() must have a // @category marker
  checkRule(
    cache,
    violations,
    "FE-100B",
    "$derived() / $derived.by() declaration without @category marker",
    srcPatterns,
    /\$derived(?:\.\w+)?\s*\(/,
    (line, index, lines) => {
      if (line.includes("@category")) return true;
      for (let i = index - 1; i >= Math.max(0, index - 3); i--) {
        const trimmed = lines[i].trim();
        if (trimmed === "") continue;
        if (trimmed.startsWith("//") && trimmed.includes("@category")) return true;
        if (trimmed.startsWith("//")) continue;
        break;
      }
      return false;
    },
    "ERROR",
    fileFilter,
  );

  // FE-100C: Module-level reactive let must have @category
  {
    const scannedFiles = new Set<string>();
    for (const pattern of srcPatterns) {
      for (const file of new Glob(pattern).scanSync(".")) {
        const normalizedFile = file.replace(/\\/g, "/");
        if (scannedFiles.has(normalizedFile)) continue;
        scannedFiles.add(normalizedFile);
        if (!fileFilter(normalizedFile)) continue;

        const content = cache.get(file);
        const lines = content.split("\n");

        for (let i = 0; i < lines.length; i++) {
          const line = lines[i];
          if (line.trim().startsWith("//")) continue;
          const letMatch = line.match(/^\s*(?:export\s+)?let\s+(\w+)\s*(?::\s*\w+\s*)?=/);
          if (!letMatch) continue;
          const varName = letMatch[1];
          const isReactive = content.includes(`$: ${varName}`) ||
            content.includes(`${varName} = $derived`) ||
            content.includes(`${varName}.subscribe`) ||
            content.includes(`$${varName}`);
          if (!isReactive) continue;
          let hasCategory = false;
          for (let j = i - 1; j >= Math.max(0, i - 3); j--) {
            const trimmed = lines[j].trim();
            if (trimmed === "") continue;
            if (trimmed.startsWith("//") && trimmed.includes("@category")) { hasCategory = true; break; }
            if (trimmed.startsWith("//")) continue;
            break;
          }
          if (!hasCategory && !line.includes("@category")) {
            violations.push({
              invariant: INVARIANT,
              severity: "WARNING",
              file,
              line: i + 1,
              message: "Module-level reactive state without @category marker",
              rule: "FE-100C",
            });
          }
        }
      }
    }
  }

  // FE-105A: writable() requires @category annotation
  checkRule(
    cache,
    violations,
    "FE-105A",
    "writable() without @category marker",
    srcPatterns,
    /writable\(/,
    (line, index, lines) => {
      if (line.includes("@category")) return true;
      for (let i = index - 1; i >= Math.max(0, index - 3); i--) {
        const trimmed = lines[i].trim();
        if (trimmed === "") continue;
        if (trimmed.startsWith("//") && trimmed.includes("@category")) return true;
        if (trimmed.startsWith("//")) continue;
        break;
      }
      return false;
    },
    "ERROR",
    fileFilter,
  );

  // FE-105B: readable() requires @category annotation
  checkRule(
    cache,
    violations,
    "FE-105B",
    "readable() without @category marker",
    srcPatterns,
    /readable\(/,
    (line, index, lines) => {
      if (line.includes("@category")) return true;
      for (let i = index - 1; i >= Math.max(0, index - 3); i--) {
        const trimmed = lines[i].trim();
        if (trimmed === "") continue;
        if (trimmed.startsWith("//") && trimmed.includes("@category")) return true;
        if (trimmed.startsWith("//")) continue;
        break;
      }
      return false;
    },
    "ERROR",
    fileFilter,
  );

  // FE-121: Every page must call createRuntimeScope() and dispose in onDestroy
  {
    for (const file of new Glob("src/pages/**/*.svelte").scanSync(".")) {
      const normalized = file.toLowerCase().replace(/\\/g, "/");
      if (normalized.includes("notfoundpage.svelte")) continue;
      if (normalized.includes("__layout")) continue;
      const content = cache.get(file);
      const scopeMatch = content.match(/const\s+(\w+)\s*=\s*createRuntimeScope\(\)/);
      const hasScope = !!scopeMatch;

      if (!hasScope) {
        if (content.includes("createRuntimeScope")) {
          violations.push({
            invariant: INVARIANT,
            severity: "ERROR",
            file,
            message: "createRuntimeScope() must be assigned to a const variable",
            rule: "FE-121",
          });
        } else {
          violations.push({
            invariant: INVARIANT,
            severity: "ERROR",
            file,
            message: "Missing createRuntimeScope()",
            rule: "FE-121",
          });
        }
        continue;
      }

      const varName = scopeMatch[1];
      const fakeDisposeRe = new RegExp(varName + '\\.dispose\\s*=\\s*(?=[^=])');
      const allLines = content.split("\n");

      for (let i = 0; i < allLines.length; i++) {
        if (fakeDisposeRe.test(allLines[i])) {
          violations.push({
            invariant: INVARIANT,
            severity: "ERROR",
            file,
            line: i + 1,
            message: `Fake dispose override detected: ${allLines[i].trim()}`,
            rule: "FE-121",
          });
        }
      }

      const allDisposeVars = [...content.matchAll(/(\w+)\.dispose\s*\(/g)].map(m => m[1]);
      const otherVars = [...new Set(allDisposeVars.filter(v => v !== varName))];
      if (otherVars.length > 0) {
        violations.push({
          invariant: INVARIANT,
          severity: "WARNING",
          file,
          message: `Dispose called on '${otherVars.join("', '")}' but createRuntimeScope assigned to '${varName}'`,
          rule: "FE-121",
        });
      }

      let disposeInOnDestroy = false;
      for (let i = 0; i < allLines.length; i++) {
        if (/onDestroy\s*\(\s*\(\s*\)\s*=>\s*\{/.test(allLines[i])) {
          let braceDepth = 0;
          let inBlock = false;
          for (let j = i; j < allLines.length; j++) {
            for (const ch of allLines[j]) {
              if (ch === "{") { braceDepth++; inBlock = true; } else if (ch === "}") { braceDepth--; }
            }
            if (inBlock && allLines[j].includes(varName + ".dispose")) { disposeInOnDestroy = true; break; }
            if (inBlock && braceDepth === 0) break;
          }
          if (disposeInOnDestroy) break;
        }
      }

      if (!disposeInOnDestroy) {
        const simpleRe = new RegExp('onDestroy\\s*\\(\\s*\\(\\s*\\)\\s*=>\\s*' + varName + '\\.dispose\\s*\\(');
        if (simpleRe.test(content)) disposeInOnDestroy = true;
      }

      if (!disposeInOnDestroy) {
        violations.push({
          invariant: INVARIANT,
          severity: "WARNING",
          file,
          message: `Has createRuntimeScope (var: ${varName}) but ${varName}.dispose() not found inside onDestroy()`,
          rule: "FE-121",
        });
      }
    }
  }

  // FE-122: createOperation/createOperationGuard should receive RuntimeScope
  checkRule(
    cache,
    violations,
    "FE-122",
    "createOperation or createOperationGuard without RuntimeScope (must pass { scope })",
    ["src/pages/**/*.svelte", "src/lib/**/*.ts"],
    /createOperation(?:Guard)?\s*\(\s*(?!\{)/,
    (line: string) => {
      if (line.trim().startsWith("//")) return true;
      if (line.match(/^\s*(export\s+)?function\s+createOperation/)) return true;
      if (line.includes("createOperation({") || line.includes("createOperationGuard({")) return true;
      if (line.includes("import ")) return true;
      if (line.includes("type CreateOperation")) return true;
      return false;
    },
    "WARNING",
    (f) => !f.includes("/tests/") && !f.includes(".test.") && !f.includes(".spec."),
  );

  // FE-149: session.ts must use createRuntimeScope for all timers and listeners
  {
    try {
      const sessionFile = "src/lib/session.ts";
      const content = cache.get(sessionFile);
      const fe149Errors: string[] = [];
      if (!content.includes("createRuntimeScope")) fe149Errors.push("missing createRuntimeScope import/usage");
      if (!content.includes("scope.setInterval") && !content.includes("scope.setTimeout")) {
        fe149Errors.push("no scope timer methods found");
      }
      if (!content.includes("scope.addListener")) fe149Errors.push("no scope.addListener found");
      if (fe149Errors.length > 0) {
        for (const e of fe149Errors) {
          violations.push({
            invariant: INVARIANT,
            severity: "ERROR",
            file: sessionFile,
            message: `session.ts violates RuntimeScope requirements: ${e}`,
            rule: "FE-149",
          });
        }
      }
    } catch { /* file may not exist */ }
  }

  // FE-150: No module-level mutable timer/listener references outside RuntimeScope
  checkRule(
    cache,
    violations,
    "FE-150",
    "Module-level mutable timer/listener reference (use RuntimeScope instead)",
    ["src/lib/**/*.ts", "src/lib/**/*.js"],
    /^\s*export\s+let\s+\w+\s*[?]?\s*:\s*(number|null\s*\|?\s*number|ReturnType<typeof\s+set(?:Timeout|Interval)>)/,
    (line: string) => {
      if (line.trim().startsWith("//")) return true;
      if (line.includes("scope.")) return true;
      return false;
    },
    "ERROR",
    (f) => {
      const n = f.toLowerCase().replace(/\\/g, "/");
      if (n.includes("/tests/") || n.includes(".test.") || n.includes(".spec.")) return false;
      return true;
    },
  );

  // FE-151: Touch/scroll/wheel listeners should use { passive: true }
  checkRule(
    cache,
    violations,
    "FE-151",
    "addEventListener with touch/scroll/wheel missing { passive: true }",
    ["src/**/*.svelte", "src/**/*.ts", "src/**/*.js"],
    /\.addEventListener\s*\(\s*(['"`])(touch|scroll|wheel)\1/,
    (line: string) => {
      if (line.trim().startsWith("//")) return true;
      if (line.includes("passive: true")) return true;
      if (/scope\.addListener/.test(line)) return true;
      return false;
    },
    "WARNING",
    fileFilter,
  );

  // Suppression metadata validation (FE-149 + FE-162)
  {
    const fePatterns = ["fe141", "fe142", "fe143", "fe145", "fe146", "fe147", "fe148"];
    const glob = new Glob("src/**/*.{svelte,ts,js}");
    const allTags: Array<{ tag: string; file: string; line: number; justification: string }> = [];

    for (const file of glob.scanSync(".")) {
      if (file.includes("/tests/") || file.includes(".test.") || file.includes(".spec.") || file.includes("/e2e/")) continue;
      const content = cache.get(file);
      const lines = content.split("\n");

      for (let i = 0; i < lines.length; i++) {
        for (const pat of fePatterns) {
          const marker = `[arch:allow-${pat}]`;
          if (lines[i].includes(marker)) {
            const tagIdx = lines[i].indexOf(marker);
            const beforeTag = lines[i].substring(0, tagIdx).replace(/^\s*\/\/\s*/, "").replace(/^\s*<!--\s*/, "").trim();
            const afterTag = lines[i].substring(tagIdx + marker.length).replace(/-->\s*$/, "").trim();
            const justification = afterTag || beforeTag;
            allTags.push({ tag: pat, file, line: i + 1, justification });

            // FE-162: Check metadata
            const hasReason = /\bReason\s*:/i.test(lines[i]);
            const hasDate = /\bDate\s*:\s*\d{4}-\d{2}-\d{2}/.test(lines[i]);
            const hasOwner = /\bOwner\s*:/i.test(lines[i]);
            if (!hasReason || !hasDate || !hasOwner) {
              violations.push({
                invariant: INVARIANT,
                severity: "ERROR",
                file,
                line: i + 1,
                message: `Suppression '${marker}' missing metadata. Required: Reason, Date, Owner`,
                rule: "FE-162",
              });
            } else {
              const dateMatch = lines[i].match(/Date\s*:\s*(\d{4}-\d{2}-\d{2})/);
              if (dateMatch) {
                const date = new Date(dateMatch[1]);
                const now = new Date();
                const diffMs = now.getTime() - date.getTime();
                const diffDays = diffMs / (1000 * 60 * 60 * 24);
                if (diffDays > 90) {
                  violations.push({
                    invariant: INVARIANT,
                    severity: "ERROR",
                    file,
                    line: i + 1,
                    message: `Suppression '${marker}' expired (${Math.floor(diffDays)} days old, max 90)`,
                    rule: "FE-162",
                  });
                }
              }
            }
          }
        }
      }
    }

    // FE-149: Empty justification
    for (const t of allTags) {
      if (!t.justification) {
        violations.push({
          invariant: INVARIANT,
          severity: "ERROR",
          file: t.file,
          line: t.line,
          message: `Empty suppression — (${t.tag}) — must include justification text`,
          rule: "FE-149",
        });
      }
    }

    // FE-149: Duplicate consecutive identical tags
    for (let i = 1; i < allTags.length; i++) {
      const a = allTags[i - 1];
      const b = allTags[i];
      if (a.tag === b.tag && a.justification === b.justification && a.file === b.file && Math.abs(a.line - b.line) <= 2) {
        violations.push({
          invariant: INVARIANT,
          severity: "ERROR",
          file: b.file,
          line: b.line,
          message: `Duplicate suppression — (${b.tag}) — same tag+justification on adjacent line`,
          rule: "FE-149",
        });
      }
    }
  }

  // Rust suppression metadata validation (FE-149 + FE-162 + ADR linkage)
  // Routes all [arch:allow-*] tags in src-tauri/src through the same lifecycle
  // policy as frontend tags (B3-2): empty/duplicate justification, mandatory
  // Reason/Date/Owner, 90-day expiry, and ADR reference.
  {
    const rustTagPatterns = [
      "utc-now",
      "mutation-before-replay",
      "unwrap-or",
      "non-nested",
      "sql",
      "memory-unsafe",
    ];
    const rustEntries = collectSuppressions(cache, ["src-tauri/src/**/*.rs"], rustTagPatterns);

    violations.push(...validateSuppressionMetadata(rustEntries, INVARIANT));

    for (const entry of rustEntries) {
      if (!/\bsee\s+ADR-\d+/i.test(entry.justification)) {
        violations.push({
          invariant: INVARIANT,
          severity: "ERROR",
          file: entry.file,
          line: entry.line,
          message: `Suppression '[arch:allow-${entry.tag}]' missing ADR reference (must include 'see ADR-NNNN')`,
          rule: "FE-162",
        });
      }
    }
  }

  return violations;
}
