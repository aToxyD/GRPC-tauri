# GRPC-Tauri Release Baseline v1.2.0

## Release Information

Version: v1.2.0
Status: Certified
Release Date: 2026-06-03

## Governance

Governance Version: v2
Governance Status: Frozen
Active Invariants: 5
Governance Errors: 0
Governance Warnings: 0

## Architecture

ADR Count:

* Existing ADRs (0001–0020, 0028–0031)
* Migrated ADRs 0021–0027
* ADR-0031 Rate Limiter Persistence

Architecture Status: Certified

## Validation

| Check | Result |
|-------|--------|
| cargo fmt --check | PASS |
| cargo clippy --all-targets -- -D warnings | PASS |
| cargo test | PASS |
| bun run check | PASS |
| bun run check:arch | PASS |

## Repository State

| Component | Status |
|-----------|--------|
| Contracts | Certified |
| Projection Ownership | Certified |
| Governance Snapshots | Certified |
| Observability | Enabled |

## Notes

v1.2.0 represents the first governance-frozen architecture baseline.

Future releases must preserve the certified invariants and governance model.
