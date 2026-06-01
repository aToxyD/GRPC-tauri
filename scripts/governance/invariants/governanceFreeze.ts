import { readFileSync } from "fs";
import type { Violation } from "../types";
import type { FileCache } from "../scanner";
import { validateSnapshotStructure, validateApprovals } from "../snapshot";

const INVARIANT = "GOVERNANCE_FREEZE" as const;
const SNAPSHOT_DIR = "docs/governance/frontend/baselines";

export function scanGovernanceFreeze(cache: FileCache): Violation[] {
  const violations: Violation[] = [];

  // FE-165: Release Gate — required governance artifacts exist
  {
    const requiredSnapshots = [
      "contracts.snapshot.json", "domain-ownership.snapshot.json",
      "projection-ownership.snapshot.json", "import-graph.snapshot.json",
    ];

    for (const snap of requiredSnapshots) {
      try {
        JSON.parse(cache.get(`${SNAPSHOT_DIR}/${snap}`));
      } catch {
        violations.push({
          invariant: INVARIANT,
          severity: "ERROR",
          file: `${SNAPSHOT_DIR}/${snap}`,
          message: `Release gate — governance snapshot missing or invalid: ${snap}`,
          rule: "FE-165",
        });
      }
    }

    const requiredDocs: Array<{ path: string; check: (c: string) => boolean }> = [
      { path: "docs/governance/frontend/GOVERNANCE_APPROVALS.md", check: (c) => c.includes("## Approvals") },
      { path: "docs/governance/frontend/GOVERNANCE_FREEZE.md", check: (_c) => true },
      { path: "docs/governance/frontend/RELEASE_CERTIFICATION_CHECKLIST.md", check: (_c) => true },
      { path: "docs/governance/frontend/GOVERNANCE_METRICS.md", check: (_c) => true },
      { path: "docs/governance/frontend/GOVERNANCE_COVERAGE_REPORT.md", check: (c) => c.includes("Coverage Metrics") },
      { path: "docs/governance/frontend/FRONTEND_CERTIFICATION_v6.md", check: (c) => c.includes("Status:") },
    ];

    for (const doc of requiredDocs) {
      try {
        const content = readFileSync(doc.path, "utf-8");
        if (!doc.check(content)) {
          violations.push({
            invariant: INVARIANT,
            severity: "ERROR",
            file: doc.path,
            message: `Release gate — missing required content`,
            rule: "FE-165",
          });
        }
      } catch {
        violations.push({
          invariant: INVARIANT,
          severity: "ERROR",
          file: doc.path,
          message: `Release gate — file missing`,
          rule: "FE-165",
        });
      }
    }
  }

  // FE-166: Snapshot Approval Enforcement
  {
    const snapshots = [
      { file: "contracts.snapshot.json" },
      { file: "domain-ownership.snapshot.json" },
      { file: "projection-ownership.snapshot.json" },
      { file: "import-graph.snapshot.json" },
    ];

    violations.push(...validateSnapshotStructure(cache, SNAPSHOT_DIR, snapshots, INVARIANT));
    violations.push(...validateApprovals(cache, INVARIANT));
  }

  // FE-167: Certification Consistency
  {
    const markdownFiles = [
      {
        path: "docs/governance/frontend/FRONTEND_CERTIFICATION_v6.md",
        checks: [
          { keyword: "v5-freeze", message: "does not reference snapshot version 'v5-freeze'" },
          { keyword: "v6", message: "does not reference version 'v6'" },
        ],
      },
      {
        path: "docs/governance/frontend/GOVERNANCE_FREEZE.md",
        checks: [
          { keyword: "v5", message: "does not reference version 'v5'" },
        ],
      },
      {
        path: "docs/governance/frontend/GOVERNANCE_METRICS.md",
        checks: [
          { keyword: "v5-freeze", message: "does not reference snapshot version 'v5-freeze'" },
        ],
      },
      {
        path: "docs/governance/frontend/GOVERNANCE_APPROVALS.md",
        checks: [
          { keyword: "v5", message: "does not reference version v5 or Phase 5" },
        ],
      },
    ];

    for (const { path, checks } of markdownFiles) {
      try {
        const content = readFileSync(path, "utf-8");
        for (const check of checks) {
          if (!content.includes(check.keyword)) {
            violations.push({
              invariant: INVARIANT,
              severity: "WARNING",
              file: path,
              message: check.message,
              rule: "FE-167",
            });
          }
        }
      } catch { /* handled by FE-165 */ }
    }

    const certFiles = [
      "docs/governance/frontend/archive/FRONTEND_CERTIFICATION_v1.md",
      "docs/governance/frontend/archive/FRONTEND_CERTIFICATION_v2.md",
      "docs/governance/frontend/archive/FRONTEND_CERTIFICATION_v3.md",
      "docs/governance/frontend/archive/FRONTEND_CERTIFICATION_v4.md",
      "docs/governance/frontend/archive/FRONTEND_CERTIFICATION_v5.md",
    ];

    for (const f of certFiles) {
      try {
        readFileSync(f, "utf-8");
      } catch {
        violations.push({
          invariant: INVARIANT,
          severity: "WARNING",
          file: f,
          message: "Archived certification file missing",
          rule: "FE-167",
        });
      }
    }
  }

  return violations;
}
