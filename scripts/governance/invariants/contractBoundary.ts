import { Glob } from "bun";
import type { Violation } from "../types";
import type { FileCache } from "../scanner";

const INVARIANT = "CONTRACT_BOUNDARY" as const;

export function scanContractBoundary(cache: FileCache): Violation[] {
  const violations: Violation[] = [];
  const contractGlob = new Glob("src/lib/contracts/*.contract.ts");
  const expectedContracts = [
    "consumption", "inventory", "report", "session", "user",
    "orders", "sync", "backup", "metrics", "audit",
    "observability", "fiscal", "dashboard", "platform",
  ];

  // FE-111: All contract files must exist
  const existingContracts = new Set(
    [...contractGlob.scanSync(".")]
      .map((f) => f.replace(/^.*\/(\w+)\.contract\.ts$/, "$1")),
  );
  const missing = expectedContracts.filter((d) => !existingContracts.has(d));
  if (missing.length > 0) {
    violations.push({
      invariant: INVARIANT,
      severity: "ERROR",
      file: "src/lib/contracts/",
      message: `Required contract files missing for domains: ${missing.join(", ")}. Target: src/lib/contracts/<domain>.contract.ts`,
      rule: "FE-111",
    });
  }

  // FE-136: Contract barrel must exist
  if (![...new Glob("src/lib/contracts/index.ts").scanSync(".")].length) {
    violations.push({
      invariant: INVARIANT,
      severity: "ERROR",
      file: "src/lib/contracts/index.ts",
      message: "Required contract barrel file missing: src/lib/contracts/index.ts",
      rule: "FE-136",
    });
  }

  // FE-112: Contract file must export at least one safeInvoke wrapper
  let hasFe112Warning = false;
  for (const file of contractGlob.scanSync(".")) {
    const content = cache.get(file);
    if (!content.includes("safeInvoke")) {
      violations.push({
        invariant: INVARIANT,
        severity: "WARNING",
        file,
        message: "Does not export any safeInvoke wrapper",
        rule: "FE-112",
      });
      hasFe112Warning = true;
    }
  }

  // FE-114: Every exported contract function must have explicit typed return value
  for (const file of contractGlob.scanSync(".")) {
    const content = cache.get(file);
    const lines = content.split("\n");
    for (let i = 0; i < lines.length; i++) {
      const line = lines[i];
      if (line.trim().startsWith("//")) continue;

      // Function declaration
      if (/export\s+(async\s+)?function\s+\w+\s*\([^)]*\)\s*(?!:\s*Promise)/.test(line)) {
        violations.push({
          invariant: INVARIANT,
          severity: "ERROR",
          file,
          line: i + 1,
          message: "Contract function missing explicit typed return value (must declare : Promise<...>)",
          rule: "FE-114",
        });
      }

      // Arrow function
      if (/export\s+(const|let|var)\s+\w+\s*=\s*async\s*\([^)]*\)\s*(?!:\s*Promise)/.test(line)) {
        violations.push({
          invariant: INVARIANT,
          severity: "ERROR",
          file,
          line: i + 1,
          message: "Contract arrow function missing explicit typed return value (must declare : Promise<...>)",
          rule: "FE-114",
        });
      }
    }
  }

  // FE-116: Contracts must not import other contracts
  for (const file of contractGlob.scanSync(".")) {
    const content = cache.get(file);
    const lines = content.split("\n");
    for (let i = 0; i < lines.length; i++) {
      const line = lines[i];
      if (line.trim().startsWith("//")) continue;
      if (/from\s+['"]\.\/(\w+)\.contract['"]|from\s+['"]\.\.\/contracts\//.test(line)) {
        violations.push({
          invariant: INVARIANT,
          severity: "ERROR",
          file,
          line: i + 1,
          message: "Contract importing another contract (contracts must be isolated)",
          rule: "FE-116",
        });
      }
    }
  }

  // FE-120: Contracts must not import Svelte runtime
  for (const file of contractGlob.scanSync(".")) {
    const content = cache.get(file);
    const lines = content.split("\n");
    for (let i = 0; i < lines.length; i++) {
      const line = lines[i];
      if (line.trim().startsWith("//")) continue;
      if (/from\s+['"]svelte/.test(line)) {
        violations.push({
          invariant: INVARIANT,
          severity: "ERROR",
          file,
          line: i + 1,
          message: "Contract importing Svelte runtime (contracts must be runtime-independent)",
          rule: "FE-120",
        });
      }
    }
  }

  // FE-138: Components must not access IPC directly
  for (const pattern of ["src/components/**/*.svelte", "src/lib/components/**/*.svelte"]) {
    for (const file of new Glob(pattern).scanSync(".")) {
      const content = cache.get(file);
      const lines = content.split("\n");
      for (let i = 0; i < lines.length; i++) {
        const line = lines[i];
        if (line.trim().startsWith("//") || line.trim().startsWith("<!--")) continue;
        if (line.includes("[arch:allow-component-ipc]")) continue;
        if (line.includes("[arch:allow-component-ipc-ghost]")) continue;
        if (i > 0 && lines[i - 1].includes("[arch:allow-component-ipc]")) continue;
        if (i > 0 && lines[i - 1].includes("[arch:allow-component-ipc-ghost]")) continue;
        if (/from\s+['"]\.\.?\/lib\/(tauri|contracts)/.test(line)) {
          violations.push({
            invariant: INVARIANT,
            severity: "ERROR",
            file,
            line: i + 1,
            message: "Component accessing IPC directly (must receive data via props from pages)",
            rule: "FE-138",
          });
        }
      }
    }
  }

  // FE-153: Contract Ownership Enforcement — each IPC command in exactly one contract
  const cmdToContracts = new Map<string, string[]>();
  for (const file of contractGlob.scanSync(".")) {
    const content = cache.get(file);
    const matches = [...content.matchAll(/safeInvoke(?:<\s*\w+(?:\s*\[\s*\w+\s*(?:,\s*\w+\s*)?\]\s*)?\s*>)?\s*\(\s*'([^']+)'/g)];
    for (const m of matches) {
      const cmd = m[1];
      if (!cmdToContracts.has(cmd)) cmdToContracts.set(cmd, []);
      cmdToContracts.get(cmd)!.push(file);
    }
  }
  for (const [cmd, contracts] of cmdToContracts) {
    if (contracts.length > 1) {
      violations.push({
        invariant: INVARIANT,
        severity: "ERROR",
        file: contracts.join(", "),
        message: `IPC command '${cmd}' appears in multiple contracts`,
        rule: "FE-153",
      });
    }
  }

  // FE-154: Barrel Integrity
  const barrelPath = "src/lib/contracts/index.ts";
  let barrelExists = false;
  try {
    const barrelContent = cache.get(barrelPath);
    barrelExists = true;
    for (const file of contractGlob.scanSync(".")) {
      const contractName = file.replace(/^.*\/(\w+)\.contract\.ts$/, "$1");
      if (!barrelContent.includes(contractName)) {
        violations.push({
          invariant: INVARIANT,
          severity: "ERROR",
          file: barrelPath,
          message: `${contractName}.contract.ts not re-exported from ${barrelPath}`,
          rule: "FE-154",
        });
      }
    }
  } catch { /* barrel missing — handled by FE-136 */ }

  // FE-154: No direct contract imports bypassing barrel
  if (barrelExists) {
    const barrelBypassPattern = /from\s+['"].*\/contracts\/\w+\.contract['"]/;
    for (const pattern of ["src/pages/**/*.svelte", "src/{components,lib}/**/*.{svelte,ts}"]) {
      for (const file of new Glob(pattern).scanSync(".")) {
        if (file.includes("/tests/") || file.includes(".test.") || file.includes(".spec.")) continue;
        const content = cache.get(file);
        const directImport = content.match(barrelBypassPattern);
        if (directImport) {
          violations.push({
            invariant: INVARIANT,
            severity: "ERROR",
            file,
            message: `Bypasses barrel — direct contract import: ${directImport[1]}`,
            rule: "FE-154",
          });
        }
      }
    }
  }

  return violations;
}
