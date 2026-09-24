# ADR 0054: Frontend Projection & Presentation Exception Governance (FE-141 / FE-146)

## Status

**Status: Accepted**

**Ratification Date: 2026-09-01**

**ACCEPTED — 2026-09-01 (Owner Ratification — Governance Gate, SEC-064 design approval / SEC-064 implementation).**

## Date

2026-08-30

## Owner

Architecture (owner assignment confirmed on approval)

## Reference

- ADR-0030 (`docs/architecture/0030-adr-exception-governance.md`) — exception
  registration/expiry/renewal policy.
- `docs/architecture/ARCHITECTURAL_INVARIANTS.md` §3 — approved exception process.
- `scripts/governance/invariants/projectionIntegrity.ts` — FE-141 / FE-146 rule
  definitions and the FE-141 scanner CSS false-positive fix (this decision, §Decision).
- `scripts/governance/suppression.ts` — FE-162 suppression metadata/lifecycle enforcement.
- `docs/architecture/adr_exception_registry.md` — exception registry (this decision adds
  the `[arch:allow-fe141]` / `[arch:allow-fe146]` section).

## Context

The FE-141 ("Division on projection values") and FE-146 ("Average recomputation")
scanner rules in `projectionIntegrity.ts` use syntactic regexes that also match
legitimate **non-business** frontend patterns:

- presentation-only percentage / progress calculations (FE-141);
- projection-shape type fields (FE-146);
- infrastructure telemetry metrics (FE-146);
- CSS opacity slash syntax (FE-141).

Six sites carried `[arch:allow-fe141]` / `[arch:allow-fe146]` tags introduced
(`commit d5348b0`, 2026-06-01) but **never registered** per ADR-0030, and have since
exceeded the 90-day lifecycle (FE-162). SEC-061/062/063 established that these are
pre-existing, unrelated to SEC-057/SEC-060, and represent presentation/infrastructure
patterns that remain intentionally permissible, but that direct renewal is not
authorized without an ADR + registry registration per ADR-0030 and
`ARCHITECTURAL_INVARIANTS.md` §3.

## Decision

The five legitimate non-business frontend uses below are registered as **time-limited**
architectural exceptions (90-day lifecycle per `ARCHITECTURAL_INVARIANTS.md` §3.3),
gated by `[arch:allow-fe141]` / `[arch:allow-fe146]` tags and ADR-0054 registry rows:

| Rule   | File | Line | Use | Classification |
| ------ | ---- | ---: | --- | -------------- |
| FE-146 | `src/components/consumption/MealSection.svelte` | 9  | `mealAverage:` type field mirroring backend projection shape | presentation type mirror |
| FE-146 | `src/components/consumption/MealSummaryCard.svelte` | 4 | `mealAverage:` type field mirroring backend projection shape | presentation type mirror |
| FE-146 | `src/lib/telemetry.ts` | 154 | telemetry latency average | infrastructure metric (not business logic) |
| FE-141 | `src/pages/SyncTopologyPage.svelte` | 201 | UI percentage progress bar | presentation-only |
| FE-141 | `src/pages/SyncTopologyPage.svelte` | 217 | UI severity percentage bar | presentation-only |

These exceptions exist **only** to distinguish legitimate non-business frontend syntax
from genuine violations:

- presentation calculation ≠ business calculation (FE-141 percentage/progress bars);
- telemetry metric ≠ business calculation (FE-146 latency average);
- TypeScript/Svelte type-field name ≠ business computation (FE-146 `mealAverage:`).

## Explicit Exclusions (NOT exceptions)

`src/pages/UnitDashboard.svelte:131` (`bg-red-50/50`) is a **scanner false-positive**:
the `/` is CSS opacity syntax, not arithmetic division. It MUST **not** be registered as
an exception. Instead the FE-141 scanner regex is narrowed (in
`projectionIntegrity.ts`) to `/\/(?!\d)(?=[^;]*\b(...)\b)/i` so a `/` immediately
followed by a digit (CSS opacity `/50`, `/10`) is no longer classified as FE-141. This
is a minimal, lookbehind-independent narrowing that preserves genuine identifier-based
division detection and cannot disable FE-141 detection elsewhere.

## Scope

**In scope:** the five frontend sites listed above (tags + registry rows referencing
ADR-0054) and the FE-141 scanner false-positive remediation.

**Out of scope (remains forbidden):** any backend business logic in the frontend;
authority decisions; credential validation; trust decisions; synchronization ordering;
package acceptance; replay/version mechanisms; data-integrity decisions; any
`[arch:allow-*]` beyond the five listed; any change to SEC-057/SEC-060 semantics.

## Security Boundary

These exceptions do **not** authorize:

- backend business computation in frontend layers;
- synchronization ordering or sequence/revision/version mechanisms;
- replay/package-acceptance semantics;
- trust, identity, credential, or package security decisions;
- any deviation in admin_access, identity_access, monthly completeness, products LWW,
  trust CredentialGuard, or registry semantics.

They have **no relationship** to SEC-057 or SEC-060 semantics.

## Review / Expiry

All five registered exceptions expire 90 days after registration and must be renewed
through the repository's review/approval process (fresh technical justification and
full review, `ARCHITECTURAL_INVARIANTS.md` §3.3; quarterly window). A later decision
may propose permanent status for the recurring presentation patterns only through the
RFC-to-ADR process and `PERMANENT_ADRS` registration (precedent: ADR-0043). This ADR
does **not** mark the exceptions Permanent.

## Alternatives (rejected)

- **Permanent now:** rejected — `ARCHITECTURAL_INVARIANTS.md` §3.3 defaults exceptions
  to time-limited; permanent requires the RFC-to-ADR + freeze amendment +
  `PERMANENT_ADRS` seeding (ADR-0043 precedent), heavier than justified here.
- **Remove all tags without remediation:** rejected — re-opens five gate errors at the
  legitimate presentation sites.
- **Keep unregistered:** rejected — violates ADR-0030 (undocumented exemptions are
  violations) and `ARCHITECTURAL_INVARIANTS.md` §3.
- **Register the CSS false-positive:** rejected — it is not a genuine division; the
  correct fix is the scanner narrowing (§Decision).

## Consequences

- Zero undocumented exceptions for these sites (ADR-0030 satisfied).
- FE-162 lifecycle enforced for the five registered sites.
- UnitDashboard CSS false-positive resolved at the scanner level (no suppression).
- No impact on SEC-057/SEC-060 semantics, synchronization, or backend security.

## Backward Compatibility

Zero behavior change to application code. The tags are comment-only; the scanner
narrowing removes a false-positive classification only and cannot introduce new
violations (it strictly reduces the set of matched lines).
