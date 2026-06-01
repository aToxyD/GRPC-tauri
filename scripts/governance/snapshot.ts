import type { Violation } from "./types";
import type { FileCache } from "./scanner";
import { Glob } from "bun";

export function loadSnapshot(cache: FileCache, path: string): any {
  const content = cache.get(path);
  return JSON.parse(content);
}

export function validateSnapshotsExist(
  cache: FileCache,
  snapshotDir: string,
  snapshots: string[],
  invariant: Violation["invariant"],
): Violation[] {
  const violations: Violation[] = [];
  for (const snap of snapshots) {
    try {
      loadSnapshot(cache, `${snapshotDir}/${snap}`);
    } catch {
      violations.push({
        invariant,
        severity: "ERROR",
        file: `${snapshotDir}/${snap}`,
        message: `Governance snapshot missing or invalid: ${snap}`,
        rule: "FE-158",
      });
    }
  }
  return violations;
}

export function compareContractsSnapshot(
  cache: FileCache,
  snapshotDir: string,
  invariant: Violation["invariant"],
): Violation[] {
  const violations: Violation[] = [];

  let contractSnap: any;
  try {
    contractSnap = loadSnapshot(cache, `${snapshotDir}/contracts.snapshot.json`);
  } catch {
    return violations;
  }

  const currentContracts: Record<string, { exports: string[]; loc: number }> = {};
  for (const file of new Glob("src/lib/contracts/*.contract.ts").scanSync(".")) {
    const content = cache.get(file);
    const name = file.split("/").pop()!.replace(".contract.ts", "");
    const exports = [...content.matchAll(/export\s+async\s+function\s+(\w+)/g)].map((m) => m[1]);
    currentContracts[name] = { exports, loc: content.split("\n").length };
  }

  for (const snapContract of contractSnap.contracts || []) {
    const current = currentContracts[snapContract.name];
    if (!current) {
      violations.push({
        invariant,
        severity: "ERROR",
        file: `${snapshotDir}/contracts.snapshot.json`,
        message: `Contract '${snapContract.name}' exists in snapshot but not in current codebase`,
        rule: "FE-158",
      });
      continue;
    }
    for (const exportEntry of snapContract.exports || []) {
      if (!current.exports.includes(exportEntry.name)) {
        violations.push({
          invariant,
          severity: "ERROR",
          file: `${snapshotDir}/contracts.snapshot.json`,
          message: `Contract '${snapContract.name}' lost export '${exportEntry.name}' (governance drift)`,
          rule: "FE-158",
        });
      }
    }
  }

  for (const name of Object.keys(currentContracts)) {
    if (!(contractSnap.contracts || []).some((c: any) => c.name === name)) {
      violations.push({
        invariant,
        severity: "ERROR",
        file: `${snapshotDir}/contracts.snapshot.json`,
        message: `New contract '${name}' not in governance snapshot`,
        rule: "FE-158",
      });
    }
  }

  return violations;
}

export function compareDomainOwnershipSnapshot(
  cache: FileCache,
  snapshotDir: string,
  invariant: Violation["invariant"],
): Violation[] {
  const violations: Violation[] = [];

  let domainSnap: any;
  try {
    domainSnap = loadSnapshot(cache, `${snapshotDir}/domain-ownership.snapshot.json`);
  } catch {
    return violations;
  }

  const currentPages: Record<string, string[]> = {};
  const allExports = new Set<string>();
  for (const cf of new Glob("src/lib/contracts/*.contract.ts").scanSync(".")) {
    const cc = cache.get(cf);
    for (const e of cc.matchAll(/export\s+async\s+function\s+(\w+)/g)) allExports.add(e[1]);
  }

  for (const file of new Glob("src/pages/**/*.svelte").scanSync(".")) {
    if (file.includes("/tests/")) continue;
    const content = cache.get(file);
    const name = file.split("/").pop()!.replace(".svelte", "");
    const fnMatches = [...content.matchAll(/\b(\w+)\s*\(/g)].map((m) => m[1]);
    currentPages[name] = fnMatches.filter((fn) => allExports.has(fn));
  }

  for (const [page, snapFns] of Object.entries(domainSnap.pages || {})) {
    const currentFns = currentPages[page] || [];
    for (const snapFn of snapFns as string[]) {
      if (!currentFns.includes(snapFn)) {
        violations.push({
          invariant,
          severity: "ERROR",
          file: `${snapshotDir}/domain-ownership.snapshot.json`,
          message: `Page '${page}' no longer uses function '${snapFn}' from domain snapshot`,
          rule: "FE-158",
        });
      }
    }
  }

  return violations;
}

export function compareProjectionSnapshot(
  cache: FileCache,
  snapshotDir: string,
  invariant: Violation["invariant"],
): Violation[] {
  const violations: Violation[] = [];

  let snapshot: any;
  try {
    const snapContent = cache.get(`${snapshotDir}/projection-ownership.snapshot.json`);
    snapshot = JSON.parse(snapContent);
  } catch {
    return violations;
  }

  const snapTypes = new Map(snapshot.types.map((t: any) => [t.name, t]));
  const content = cache.get("src/lib/types.ts");
  const lines = content.split("\n");
  let currentType: { name: string; kind: string; fields: string[] } | null = null;
  const currentTypes = new Map<string, { name: string; kind: string; fields: string[] }>();
  let typeAliasLine = false;

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    const ifaceMatch = line.match(/^export\s+(interface|type)\s+(\w+)/);
    if (ifaceMatch) {
      if (currentType) currentTypes.set(currentType.name, currentType);
      currentType = { name: ifaceMatch[2], kind: ifaceMatch[1], fields: [] };
      typeAliasLine = ifaceMatch[1] === "type";
      continue;
    }
    if (currentType) {
      const fieldMatch = line.match(/^\s+(\w[\w\?]*)\s*[?]?\s*:\s*(.+?);/);
      if (fieldMatch) {
        const fieldName = fieldMatch[1].replace("?", "");
        if (!["id", "created_at", "updated_at"].includes(fieldName)) {
          currentType.fields.push(fieldName);
        }
      }
      if (typeAliasLine) {
        currentTypes.set(currentType.name, currentType);
        currentType = null;
        typeAliasLine = false;
        continue;
      }
      if (/^\}/.test(line.trim()) || /^\};/.test(line.trim())) {
        currentTypes.set(currentType.name, currentType);
        currentType = null;
      }
    }
  }
  if (currentType) currentTypes.set(currentType.name, currentType);

  for (const [name, snapshotType] of snapTypes) {
    const currentDef = currentTypes.get(name);
    if (!currentDef) {
      violations.push({
        invariant,
        severity: "WARNING",
        file: `src/lib/types.ts`,
        message: `Projection type '${name}' removed from current codebase (projection mutation)`,
        rule: "FE-160",
      });
      continue;
    }
    const currentFields = currentDef.fields;
    for (const field of currentFields) {
      if (!snapshotType.fields.some((f: any) => f.name === field)) {
        violations.push({
          invariant,
          severity: "WARNING",
          file: `src/lib/types.ts`,
          message: `Projection '${name}' — new field '${field}' added (projection surface evolution)`,
          rule: "FE-160",
        });
      }
    }
    for (const snapField of snapshotType.fields) {
      if (!currentFields.includes(snapField.name)) {
        violations.push({
          invariant,
          severity: "WARNING",
          file: `src/lib/types.ts`,
          message: `Projection '${name}' — field '${snapField.name}' removed (projection mutation)`,
          rule: "FE-160",
        });
      }
    }
  }

  for (const name of currentTypes.keys()) {
    if (!snapTypes.has(name)) {
      violations.push({
        invariant,
        severity: "WARNING",
        file: `src/lib/types.ts`,
        message: `New projection type '${name}' not in governance snapshot (projection surface evolution)`,
        rule: "FE-160",
      });
    }
  }

  return violations;
}

export function validateSnapshotStructure(
  cache: FileCache,
  snapshotDir: string,
  snapshots: Array<{ file: string }>,
  invariant: Violation["invariant"],
): Violation[] {
  const violations: Violation[] = [];

  for (const snap of snapshots) {
    try {
      const parsed = JSON.parse(cache.get(`${snapshotDir}/${snap.file}`));
      if (!parsed.generated) {
        violations.push({
          invariant,
          severity: "ERROR",
          file: `${snapshotDir}/${snap.file}`,
          message: `Missing 'generated' timestamp (not a valid governance snapshot)`,
          rule: "FE-166",
        });
      }
    } catch {
      // missing snapshots handled by FE-165
    }
  }

  return violations;
}

export function validateApprovals(
  cache: FileCache,
  immutable: Violation["invariant"],
): Violation[] {
  const violations: Violation[] = [];

  try {
    const approvalsContent = cache.get("docs/governance/frontend/GOVERNANCE_APPROVALS.md");
    if (!approvalsContent.includes("snapshot") && !approvalsContent.includes("Snapshot")) {
      violations.push({
        invariant: immutable,
        severity: "ERROR",
        file: "docs/governance/frontend/GOVERNANCE_APPROVALS.md",
        message: "GOVERNANCE_APPROVALS.md contains no snapshot-related approvals",
        rule: "FE-166",
      });
    }
  } catch {
    // handled by FE-165
  }

  return violations;
}
