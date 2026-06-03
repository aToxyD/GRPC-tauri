# Documentation Audit v1.2.0 — GRPC-Tauri

## Files Removed

| Path | Reason |
|------|--------|
| Legacy ADR-001 | Migrated to `docs/architecture/0021-fifo-determinism.md` |
| Legacy ADR-002 | Migrated to `docs/architecture/0022-domain-event-ordering.md` |
| Legacy ADR-003 | Migrated to `docs/architecture/0023-reproducible-reporting.md` |
| Legacy ADR-004 | Migrated to `docs/architecture/0024-audit-dual-write.md` |
| Legacy ADR-005 | Migrated to `docs/architecture/0025-sqlite-single-writer.md` |
| Legacy ADR-006 | Migrated to `docs/architecture/0026-no-async-runtime.md` |
| Legacy ADR-007 | Migrated to `docs/architecture/0027-runtime-evaluation-purity.md` |
| Legacy ADR README | Legacy ADR directory README — no longer needed |
| `package-lock.json` | Forbidden lockfile — Bun is the package manager |
| `docs/governance/frontend/FRONTEND_CERTIFICATION_v2.md` | Duplicate — superseded by v6 |
| `docs/governance/frontend/FRONTEND_CERTIFICATION_v3.md` | Duplicate — superseded by v6 |
| `docs/governance/frontend/FRONTEND_CERTIFICATION_v4.md` | Duplicate — superseded by v6 |
| `docs/ui/FRONTEND_STABILITY_GOVERNANCE.md` | Duplicate — canonical version in `docs/frontend/` |

## Files Moved

| Source | Destination |
|--------|------------|
| `docs/ui/THEME_SYSTEM.md` | `docs/frontend/THEME_SYSTEM.md` |
| `docs/ui/UI_OPERATION_SAFETY_CONTRACT.md` | `docs/frontend/UI_OPERATION_SAFETY_CONTRACT.md` |
| `docs/sync-runbook.md` | `docs/runbooks/sync-runbook.md` |
| `docs/policies/sqlite_connection_policy.md` | `docs/architecture/sqlite_connection_policy.md` |
| `docs/releases/release_certification_process.md` | `docs/technical/release_certification_process.md` |

## Directories Removed

| Directory | Reason |
|-----------|--------|
| Legacy ADR directory | Legacy ADRs consolidated into `docs/architecture/` |
| `docs/ui/` | Contents moved to `docs/frontend/` |
| `docs/policies/` | Single file moved to `docs/architecture/` |
| `docs/releases/` | Single file moved to `docs/technical/` |

## Files Consolidated

| Source | Target | Action |
|--------|--------|--------|
| Legacy ADR-001 through ADR-007 | `docs/architecture/0021` through `0027` | Copied, legacy directory removed |
| `docs/ui/THEME_SYSTEM.md` | `docs/frontend/THEME_SYSTEM.md` | Moved |
| `docs/ui/UI_OPERATION_SAFETY_CONTRACT.md` | `docs/frontend/UI_OPERATION_SAFETY_CONTRACT.md` | Moved |

## Remaining Documentation Structure

```
docs/
├── README.md
├── architecture/           ← Active ADRs and architecture documents
├── frontend/               ← Frontend guidelines and contracts
├── governance/             ← Governance engine and observability
│   └── frontend/           ← Frontend governance docs + auto-generated snapshots
│       └── archive/        ← Historical certification versions
├── operator/               ← Operational procedures
├── runbooks/               ← Technical runbooks
├── security/               ← Security and threat-model documentation
└── technical/              ← Build, CI, glossary, release process
```

## Verification Checks

| Check | Expected | Actual |
|-------|----------|--------|
| Legacy ADR directory exists | No | ✅ Removed |
| `docs/ui/` exists | No | ✅ Removed |
| `docs/policies/` exists | No | ✅ Removed |
| `docs/releases/` exists | No | ✅ Removed |
| Duplicate cert v2/v3/v4 in `docs/governance/frontend/` | No | ✅ Removed |
| ADR 0021–0027 in `docs/architecture/` | Yes | ✅ Present |
| Moved files at canonical locations | Yes | ✅ All present |
| Stale references to removed legacy ADR directory | None | ✅ None |
| Governance files untracked in git | Yes | ✅ Untracked |
| Governance files present on disk | Yes | ✅ All on disk |

## Audit Result

**PASS**
