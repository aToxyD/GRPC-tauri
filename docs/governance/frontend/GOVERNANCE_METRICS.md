# Governance Metrics Dashboard

Generated: 2026-06-01
Snapshot: v5-freeze

---

## Current Metrics

| Metric | Value |
|--------|-------|
| Total governance rules | 47 |
| Active rules | 47 (100%) |
| ERROR severity rules | 35 |
| WARNING severity rules | 12 |
| Deferred rules | 0 |
| Active suppressions | 9 |
| Expired suppressions | 0 |
| Governance approvals | 5 |
| Architecture drift events | 0 |
| Certification versions | 5 (v1–v5) |
| Governance snapshots | 4 |
| Governance documents | 10 |

---

## Rule Distribution by Group

| Group | Rules | ERROR | WARNING |
|-------|-------|-------|---------|
| GROUP 24 — Frontend Governance (FE-100–FE-151) | 24 | 18 | 6 |
| GROUP 25 — Projection Purity (FE-141–FE-149) | 9 | 8 | 1 |
| GROUP 26 — Domain Isolation (FE-152–FE-157) | 6 | 4 | 2 |
| GROUP 27 — Continuous Governance (FE-158–FE-163) | 5 | 3 | 2 |
| GROUP 28 — Release Governance (FE-165–FE-167) | 3 | 2 | 1 |
| **Total** | **47** | **35** | **12** |

---

## Certification History

| Version | Date | Key Changes |
|---------|------|-------------|
| v1 | 2026-04-01 (est.) | Initial governance baseline |
| v2 | 2026-05-01 (est.) | Projection purity lockdown |
| v3 | 2026-05-15 (est.) | Domain isolation + contract ownership |
| v4 | 2026-06-01 | Continuous governance + snapshots |
| v5 | 2026-06-01 | Governance freeze + release certification |

---

## Snapshot History

| Snapshot | Version | Date | Status |
|----------|---------|------|--------|
| contracts.snapshot.json | v5-freeze | 2026-06-01 | Frozen |
| domain-ownership.snapshot.json | v5-freeze | 2026-06-01 | Frozen |
| projection-ownership.snapshot.json | v5-freeze | 2026-06-01 | Frozen |
| import-graph.snapshot.json | v5-freeze | 2026-06-01 | Frozen |

---

## Suppression Trends

| Version | Suppressions | Trend |
|---------|-------------|-------|
| v3 (baseline) | 9 | Baseline |
| v4 | 9 | Stable |
| v5 | 9 | Stable |

---

## Approval Summary

| ID | Date | Change | Approver |
|----|------|--------|----------|
| 1 | 2026-06-01 | Phase 4A certification baseline | governance-certification |
| 2 | 2026-06-01 | Phase 4B certification baseline | governance-certification |
| 3 | 2026-06-01 | Initial governance snapshots | governance-certification |
| 4 | 2026-06-01 | Suppression metadata annotation | governance-certification |
| 5 | 2026-06-01 | Phase 5 — Governance freeze | governance-certification |

---

## Risk Assessment

| Area | Level | Notes |
|------|-------|-------|
| Governance drift | ✅ None | FE-158 passes |
| Contract mutation | ✅ None | FE-159 passes |
| Projection mutation | ✅ None | FE-160 passes |
| Suppression expiry | ✅ None | All <90 days |
| Dead artifacts | ✅ None | FE-163 passes |
| Snapshot integrity | ✅ None | All snapshots match |
| Release readiness | ✅ Pass | All gates pass |

---

## Architecture Maturity Score

| Dimension | Score |
|-----------|-------|
| Rule Coverage | 10/10 |
| Enforcement Severity | 9/10 |
| Drift Detection | 10/10 |
| Release Governance | 10/10 |
| Documentation | 10/10 |
| **Overall Maturity** | **9.8/10** |
