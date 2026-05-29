# ADR-0028: Service Size Governance Policy

## Status

Accepted — 2026-05-29

## Context

The master execution roadmap defines a 300-line limit for services "without architectural justification." 14 files across `application/services/` and `application/sync_integrity/` exceed this limit.

## Decision

Each oversized service is categorized as:

### Acceptable with ADR

These services are complex by nature of their domain responsibility and are well-factored internally. Decomposition would add module boundary overhead without semantic benefit.

| Service | Lines | Justification |
|---------|-------|-------------|
| `sync_import_execution_service.rs` | 618 | Full import pipeline: validation, conflict detection, replay protection, persistence. Multiple named internal methods. |
| `fiscal_closure_package_service.rs` | 592 | Fiscal closure orchestration: pre-flight checks, snapshot creation, FIFO reclassification, audit trail, package export. |
| `daily_report_service.rs` | 353 | Daily report CRUD with consumption items, FIFO cost allocation, validation. Multiple meal section types. |
| `fiscal_timeline_service.rs` | 319 | Fiscal year timeline: transitions, closures, archives, period queries. Multiple projection methods. |
| `sync_import_validation_service.rs` | 328 | Import package validation: schema checks, integrity verification, idempotency. Tightly coupled validation logic. |
| `operational_anomaly_service.rs` | 361 | Anomaly detection orchestration: multiple KPI checks, threshold evaluation, audit reporting. |
| `system_health_service.rs` | 322 | System health checks: connection health, backup status, sync status, integrity status. Multiple health probes. |
| `audit_service.rs` | 377 | Audit service: log_success/log_failure, entry listing, integrity verification, long-running cleanup operations. |
| `sync_integrity/types.rs` | 436 | Type definitions for sync conflict detection, reconciliation, replay, and sequencing. Pure types module. |
| `sync_integrity/reconciliation.rs` | 427 | Reconciliation logic: conflict resolution, divergence detection, merge strategies. Complex but well-factored. |
| `sync_integrity/replay.rs` | 413 | Replay protection: transactional replay detection, audit integration, sequence gap detection. |
| `sync_integrity/conflicts.rs` | 363 | Conflict detection: stock state divergence, inventory mutation windows, stale import detection. |
| `sync_integrity/sequencing.rs` | 346 | Event sequencing: per-transaction counters, ordering guarantees, gap detection. |

### Requires Decomposition

These services would benefit from extraction of helpers or sub-modules. Deferred to future governance phases.

| Service | Lines | Notes |
|---------|-------|-------|
| `deployment_readiness_service.rs` | 454 | Readiness checks could be extracted into per-domain check modules. Deferred — zero behavior change constraint. |

## Consequences

1. All oversized services are documented with justification
2. Future architectural reviews must reference this ADR when evaluating service size
3. The 300-line guideline remains in effect for new services
4. `deployment_readiness_service.rs` is flagged for future decomposition
