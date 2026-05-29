# ADR-0030: ADR Exception Governance Policy

## Status

Accepted — 2026-05-29

## Context

26 `[arch:allow-*]` architectural exception tags exist across the codebase. Previously they lacked ADR references, creation dates, expiration dates, and owners — violating the ARCHITECTURAL_INVARIANTS.md 90-day expiration policy.

## Decision

All architectural exceptions must:

1. Be registered in `docs/architecture/adr_exception_registry.md`
2. Include ADR reference, rationale, creation date, expiration/review date, and owner
3. Expire 90 days after creation (per ARCHITECTURAL_INVARIANTS.md)
4. Be enforced by `check_arch.ts` — rules must fail if an exemption lacks ADR reference, expiration, or owner

### Tag Format

```
[arch:allow-<type>] see ADR-NNNN — brief rationale
```

### Enforcement

`check_arch.ts` will scan for `[arch:allow-*]` tags and verify:
- The tag is registered in `adr_exception_registry.md`
- The tag references a valid ADR number
- The exemption has not expired

### Review Cadence

- All exceptions are reviewed every 90 days (quarterly governance review)
- Expired exceptions without a documented extension are violations
- Exception extensions must be approved by the Architecture team

## Consequences

1. Zero undocumented exemptions
2. All exceptions have a clear audit trail and expiration date
3. CI will fail on undocumented or expired exceptions
4. Exception governance is automated, not manual
