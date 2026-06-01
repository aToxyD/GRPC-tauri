# Governance Freeze

## Certified Architecture Baseline

| Field | Value |
|-------|-------|
| Certified Architecture Version | v5.0.0 |
| Snapshot Version | v5-freeze |
| Certification Version | v5 |
| Freeze Date | 2026-06-01 |
| Expiration | Permanent (subject to ADR review) |
| Governance Model | Frozen |

---

## Freeze Scope

The following architectural artifacts are **frozen** and may not change without formal governance approval:

### Contracts
- All 14 contract files in `src/lib/contracts/*.contract.ts`
- Contract barrel (`src/lib/contracts/index.ts`)
- IPC command ownership mapping

### Projections
- All projection types in `src/lib/types.ts`
- Projection ownership mapping (contract → type)

### Domain Model
- Domain ownership registry (14 domains)
- Page-to-domain mapping
- Cross-domain exceptions

### Snapshots
- `docs/governance/frontend/baselines/contracts.snapshot.json`
- `docs/governance/frontend/baselines/domain-ownership.snapshot.json`
- `docs/governance/frontend/baselines/projection-ownership.snapshot.json`
- `docs/governance/frontend/baselines/import-graph.snapshot.json`

### Governance Rules
- All 47 active FE rules (FE-100 through FE-167)
- All suppression metadata
- All governance approval entries

---

## What Freeze Means

1. **No new contract functions** without governance approval
2. **No new projection types** without governance approval
3. **No contract function movement** between domains
4. **No snapshot modification** without approval entry in GOVERNANCE_APPROVALS.md
5. **No rule severity reduction** without ADR review
6. **No suppression addition** without metadata (Reason, Date, Owner)
7. **No governance artifact removal** without certification update

---

## Approved Exception Process

Architecture changes during freeze require:

1. Document the change in `docs/governance/frontend/GOVERNANCE_APPROVALS.md`
2. Include: Date, Change description, Rule Impact, Approver, Snapshot Version
3. Update the relevant governance snapshot(s)
4. Run full verification suite
5. All checks must pass (0 errors, 0 warnings)

---

## Change Control

| Change Type | Required Action |
|-------------|----------------|
| New IPC command | Governance approval + snapshot update |
| New projection field | Governance approval + snapshot update |
| Function moved between contracts | Governance approval + snapshot update |
| Suppression added | Metadata (Reason, Date, Owner) |
| Suppression expired (>90 days) | Review + renewal + approval |
| New governance rule | ADR + approval + certification update |
| Rule severity change | ADR + approval + certification update |

---

## Freeze Verification

The freeze is enforced by:
- FE-158: Governance drift detection against snapshots
- FE-159: Contract mutation detection
- FE-160: Projection mutation detection
- FE-165: Release gate (prevents certification of drifted architectures)
- FE-166: Snapshot approval enforcement
- FE-167: Certification consistency verification

All must pass (0 errors, 0 warnings) for certification.
