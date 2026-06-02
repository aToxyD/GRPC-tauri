# Governance Self-Audit Report

Generated: 2026-06-02T00:46:04.450Z

---

## Risk Summary

| Risk Level | Count |
|------------|-------|
| LOW RISK | 32 |
| MEDIUM RISK | 9 |
| HIGH RISK | 0 |

---

## MEDIUM RISK Items

- **Ownership Exception**: scripts/governance/scanner.ts (domain: consumption) — Cross-domain exceptions: listProducts, checkStockAvailability
- **Ownership Exception**: scripts/governance/scanner.ts (domain: inventory) — Cross-domain exceptions: exportProductsPackage, importProductsPackage, exportStockMovementsPackage, importStockMovementsPackage, exportUnitNodePackage, listSupplierOrders
- **Ownership Exception**: scripts/governance/scanner.ts (domain: orders) — Cross-domain exceptions: listProducts
- **Ownership Exception**: scripts/governance/scanner.ts (domain: report) — Cross-domain exceptions: listUnits, listDailyReports, getDailyReport, exportDailyReportPackage, exportMonthlySummaryPackage
- **Ownership Exception**: scripts/governance/scanner.ts (domain: fiscal) — Cross-domain exceptions: listProducts, getSystemHealth
- **Ownership Exception**: scripts/governance/scanner.ts (domain: sync) — Cross-domain exceptions: listUnits
- **Ownership Exception**: scripts/governance/scanner.ts (domain: dashboard) — Cross-domain exceptions: getSystemHealth, getSyncHealth
- **Ownership Exception**: scripts/governance/scanner.ts (domain: metrics) — Cross-domain exceptions: listUnits, listProducts
- **Ownership Exception**: scripts/governance/scanner.ts (domain: session) — Cross-domain exceptions: login, importUnitNodePackage

---

## LOW RISK Items

- **Suppression**: src/pages/SyncTopologyPage.svelte:201 — Tag: [arch:allow-fe141]
- **Suppression**: src/pages/SyncTopologyPage.svelte:217 — Tag: [arch:allow-fe141]
- **Suppression**: src/pages/UnitDashboard.svelte:131 — Tag: [arch:allow-fe141]
- **Suppression**: src/lib/telemetry.ts:154 — Tag: [arch:allow-fe146]
- **Suppression**: src/components/consumption/MealSection.svelte:9 — Tag: [arch:allow-fe146]
- **Suppression**: src/components/consumption/MealSummaryCard.svelte:4 — Tag: [arch:allow-fe146]
- **Approval**: GOVERNANCE_APPROVALS.md — Phase 4A certification baseline — domain isolation, contract ownership, projection purity
- **Approval**: GOVERNANCE_APPROVALS.md — Phase 4B certification baseline — continuous governance, snapshot enforcement, suppression lifecycle
- **Approval**: GOVERNANCE_APPROVALS.md — Initial governance snapshots generated for contract, domain, projection, and import graph
- **Approval**: GOVERNANCE_APPROVALS.md — All existing suppressions annotated with metadata (Reason, Date, Owner)
- **Approval**: GOVERNANCE_APPROVALS.md — Phase 5 — Governance freeze and release certification
- **Approval**: GOVERNANCE_APPROVALS.md — Governance snapshots frozen at v5-freeze baseline
- **Approval**: GOVERNANCE_APPROVALS.md — v1.2.0 release certification — 6 atomic commits, all 9 release gates pass
- **Approval**: GOVERNANCE_APPROVALS.md — Governance v2 — 47 micro-rules replaced by 5 invariants (invariant-based governance)
- **Snapshot**: docs/governance/frontend/baselines/domain-ownership.snapshot.json — Generated: 2026-06-01T19:15:05.187Z
- **Snapshot**: docs/governance/frontend/baselines/import-graph.snapshot.json — Generated: 2026-06-01T19:15:08.546Z
- **Snapshot**: docs/governance/frontend/baselines/projection-ownership.snapshot.json — Generated: 2026-06-01T19:15:02.460Z
- **Snapshot**: docs/governance/frontend/baselines/contracts.snapshot.json — Generated: 2026-06-01T19:14:59.732Z
- **Contract**: src/lib/contracts/metrics.contract.ts — 4 exports
- **Contract**: src/lib/contracts/inventory.contract.ts — 25 exports
- **Contract**: src/lib/contracts/platform.contract.ts — 0 exports
- **Contract**: src/lib/contracts/audit.contract.ts — 5 exports
- **Contract**: src/lib/contracts/fiscal.contract.ts — 11 exports
- **Contract**: src/lib/contracts/backup.contract.ts — 4 exports
- **Contract**: src/lib/contracts/sync.contract.ts — 10 exports
- **Contract**: src/lib/contracts/orders.contract.ts — 8 exports
- **Contract**: src/lib/contracts/user.contract.ts — 2 exports
- **Contract**: src/lib/contracts/session.contract.ts — 7 exports
- **Contract**: src/lib/contracts/consumption.contract.ts — 9 exports
- **Contract**: src/lib/contracts/report.contract.ts — 8 exports
- **Contract**: src/lib/contracts/dashboard.contract.ts — 2 exports
- **Contract**: src/lib/contracts/observability.contract.ts — 7 exports

---


## Observability Note

This report is informational only. No enforcement. No build failure.
