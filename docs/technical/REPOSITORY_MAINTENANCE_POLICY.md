# Repository Maintenance Policy — GRPC-Tauri

## Core Principles

1. **Generated artifacts must not be committed.** Auto-generated governance snapshots, telemetry files, and build outputs are local artifacts and must remain outside version control.

2. **Governance snapshots remain versioned.** Hand-authored governance documents (rules, certifications, ADRs) are version-controlled. Auto-generated observability outputs are not.

3. **ADRs live only in `docs/architecture/`.** No ADR may reside outside this directory. Legacy ADRs must be migrated or removed. No duplicate ADR trees are permitted.

4. **No duplicate documentation trees.** Each documentation category has exactly one canonical folder. Deprecated folders must be removed after migration.

5. **New documentation must be placed in the appropriate canonical folder:**

   | Topic | Canonical Folder |
   |-------|-----------------|
   | Architecture decisions (ADRs) | `docs/architecture/` |
   | Frontend guidelines and contracts | `docs/frontend/` |
   | Governance rules and certifications | `docs/governance/` |
   | Operational procedures and runbooks | `docs/operator/` or `docs/runbooks/` |
   | Security and threat models | `docs/security/` |
   | Build, CI, glossary, release process | `docs/technical/` |

6. **Deprecated directories must be removed after migration.** Empty or fully migrated directories shall not persist.

7. **`package-lock.json` is forbidden.** Bun is the package manager of record. Lockfiles from other package managers must not be committed.

8. **Observability outputs are local artifacts, not source artifacts.** Files under `docs/governance/frontend/` that are auto-generated (`.governance-telemetry.json`, `GOVERNANCE_TIMELINE.md`, `GOVERNANCE_TELEMETRY.md`, `GOVERNANCE_HEALTH_SCORE.md`, `CHANGE_IMPACT.md`, `DEAD_ARTIFACTS.md`, `SELF_AUDIT.md`, `GOVERNANCE_ENGINE_METRICS.md`, `GOVERNANCE_COVERAGE_REPORT.md`) must remain gitignored and untracked.

## Enforcement

Repository maintenance rules are enforced via:

* `.gitignore` entries for generated artifacts
* CI checks for unexpected tracked generated files
* Manual audit before releases
