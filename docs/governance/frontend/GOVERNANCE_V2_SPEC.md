# Governance v2 — Invariant-Based Architecture

## Overview

Governance v2 replaces ~47 individual FE micro-rules (FE-100 to FE-167) with
**4 universal invariants** and **1 meta-invariant** for governance-of-governance.

The result:
- **~91% rule reduction** (47 → 5)
- **Same enforcement power** — no safety guarantees removed
- **Faster scan performance** — fewer regex passes, shared data structures
- **Easier suppression** — invariant-level instead of rule-level

---

## The 4 Invariants

### INVARIANT A — CONTRACT_BOUNDARY

**Statement:** All IPC must flow through `/src/lib/contracts/*.contract.ts` only.

| What it enforces | Replaces |
|-----------------|----------|
| All IPC commands exist only in contract files | FE-111, FE-112 |
| Contracts are pure typed wrappers (no Svelte, no cross-contract imports) | FE-116, FE-120 |
| Every contract function has explicit typed return | FE-114 |
| No IPC leakage into UI layer (pages/components) | FE-103, FE-138, Rule 27, Rule 27b |
| safeInvoke is the only IPC entry point | FE-113 |
| Each IPC command appears in exactly one contract | FE-153 |
| All contracts are re-exported through barrel — no direct imports | FE-154 |

**Suppression:** `[arch:allow-invariant-a]`

---

### INVARIANT B — PROJECTION_INTEGRITY

**Statement:** The frontend must never perform business arithmetic.

| What it enforces | Replaces |
|-----------------|----------|
| No division on cost/average/quantity values | FE-141 |
| No multiplication on unit_cost/price/quantity | FE-142 |
| No addition on cost/total values | FE-143 |
| No consumption arithmetic (mealCount, portion, etc.) | FE-145 |
| No average recomputation | FE-146 |
| No projection reassembly (spread/Object.assign) | FE-147 |
| No long $derived computation chains (semantic leakage) | FE-148 |
| Pages consume projections only through their owning contract domain | FE-152 |
| No frontend-convenience computed/derived/helper fields in types | FE-157 |

**Suppression:** `[arch:allow-invariant-b]`

---

### INVARIANT C — RUNTIME_SAFETY

**Statement:** All reactive state and side effects must be governed.

| What it enforces | Replaces |
|-----------------|----------|
| Every `$state()` must have `// @category` marker | FE-100 |
| Every `$derived()` must have `// @category` marker | FE-100B |
| Module-level reactive `let` must have `// @category` marker | FE-100C |
| `writable()` / `readable()` must have `// @category` marker | FE-105A, FE-105B |
| Every page must call `createRuntimeScope()` and dispose in `onDestroy` | FE-121 |
| `createOperation` / `createOperationGuard` must receive `{ scope }` | FE-122 |
| `session.ts` must use RuntimeScope for all timers/listeners | FE-149 |
| No module-level mutable timer/listener references outside RuntimeScope | FE-150 |
| Touch/scroll/wheel listeners must use `{ passive: true }` | FE-151 |
| All suppressions must have valid justification (no empty/duplicate/unused) | FE-149 (suppression validation) |
| No untracked timers (`setTimeout`/`setInterval`) in pages/components | Rule 41 |
| No unmanaged `addEventListener` in pages/components | Rule 42 |
| No raw `alert()` / `confirm()` in frontend | Rule 43 |
| No silent `catch {}` blocks in pages | Rule 45 |
| No manual loading/submitting/resolving state assignments | Rule 40 |

**Suppression:** `[arch:allow-invariant-c]`

---

### INVARIANT D — ARCHITECTURE_GRAPH

**Statement:** The system's import graph and layer structure must remain stable.

| What it enforces | Replaces |
|-----------------|----------|
| Pages must not import other pages | FE-131 |
| Components must not import pages | FE-132 |
| Orphan page/contract detection | FE-155 |
| Contract size governance (max 50 exports, max 500 LOC) | FE-156 |
| Snapshot drift — contract exports lost or added without approval | FE-158 |
| Contract mutation — functions duplicated across contracts | FE-159 |
| Projection mutation — type fields added/removed vs snapshot | FE-160 |
| Suppression lifecycle — metadata required, 90-day expiry | FE-162 |
| Dead governance artifacts — unused registry entries, unused suppressions | FE-163 |

**Suppression:** `[arch:allow-invariant-d]`

---

## Meta-Invariant: GOVERNANCE_FREEZE

**Statement:** The governance system itself is frozen and externally verifiable.

| What it enforces | Replaces |
|-----------------|----------|
| All governance documents exist and are valid | FE-165 |
| Snapshots reference GOVERNANCE_APPROVALS.md | FE-166 |
| Certification versions are synchronized across docs | FE-167 |

This is not a normal invariant — it is a **meta-check** that validates the
governance infrastructure itself. It runs separately from the 4 invariants
and is always enforced at ERROR/WARNING level.

**Suppression:** Not suppressible.

---

## Rule Reduction Summary

| Governance Layer | Old Rules | New Invariants | Reduction |
|-----------------|-----------|----------------|-----------|
| Contract Boundary | ~12 rules | 1 | ~92% |
| Projection Integrity | ~11 rules | 1 | ~91% |
| Runtime Safety | ~16 rules | 1 | ~94% |
| Architecture Graph | ~8 rules | 1 | ~88% |
| Governance Freeze (meta) | ~3 rules | 1 | ~67% |
| **Total** | **~50 micro-rules** | **5 invariants** | **~90%** |

Backend Rust rules (Groups 1–23, ~80+ rules) remain unchanged.

---

## Migration Compatibility

- All existing `[arch:allow-fe*]` suppressions continue to work (mapped to invariant-level)
- `[arch:allow-invariant-*]` is the new recommended format
- Old `[arch:allow-fe141]` → `[arch:allow-invariant-b]` (equivalent)
- No contract API changes required
- No backend changes required
- No application logic changes required

---

## Mapping: Old FE Rules → Invariant

| Old Rule | New Invariant |
|----------|--------------|
| FE-100, FE-100B, FE-100C | C — RUNTIME_SAFETY |
| FE-103 | A — CONTRACT_BOUNDARY |
| FE-105A, FE-105B | C — RUNTIME_SAFETY |
| FE-111, FE-112, FE-113, FE-114 | A — CONTRACT_BOUNDARY |
| FE-116 | A — CONTRACT_BOUNDARY |
| FE-120 | A — CONTRACT_BOUNDARY |
| FE-121, FE-122 | C — RUNTIME_SAFETY |
| FE-138 | A — CONTRACT_BOUNDARY |
| FE-141, FE-142, FE-143, FE-145 | B — PROJECTION_INTEGRITY |
| FE-146, FE-147, FE-148 | B — PROJECTION_INTEGRITY |
| FE-149 (suppression validation) | C — RUNTIME_SAFETY |
| FE-150, FE-151 | C — RUNTIME_SAFETY |
| FE-152 | B — PROJECTION_INTEGRITY |
| FE-153, FE-154 | A — CONTRACT_BOUNDARY |
| FE-155, FE-156 | D — ARCHITECTURE_GRAPH |
| FE-157 | B — PROJECTION_INTEGRITY |
| FE-158, FE-159, FE-160 | D — ARCHITECTURE_GRAPH |
| FE-162, FE-163 | D — ARCHITECTURE_GRAPH |
| FE-165, FE-166, FE-167 | META — GOVERNANCE_FREEZE |
| Rule 27, Rule 27b | A — CONTRACT_BOUNDARY |
| Rule 40, Rule 41, Rule 42, Rule 43, Rule 45 | C — RUNTIME_SAFETY |

---

## Preserved Guarantees

- ✅ No business arithmetic in frontend
- ✅ No IPC leakage outside contracts
- ✅ No unauthorized cross-contract imports
- ✅ All reactive state categorized
- ✅ RuntimeScope lifecycle enforced on all pages
- ✅ Snapshot-based governance drift detection
- ✅ Contract ownership enforcement (1 function = 1 contract)
- ✅ Barrel integrity (no direct contract imports)
- ✅ Projection purity (no frontend recomputation)
- ✅ Release gate (all artifacts must exist)
- ✅ Certification consistency (version sync across docs)
- ✅ Suppression lifecycle (metadata + expiry)
- ✅ Dead artifact detection
- ✅ Backend authority preserved
- ✅ Determinism preserved
- ✅ Auditability preserved
