# ADR-0031 — Rate Limiter Persistence Across Restarts

**Status:** Accepted
**Date:** 2026-06-03
**Layer:** Infrastructure / Security

## Context

The original in-memory rate limiter reset all failed-login attempt counts
whenever the application restarted.

An attacker could bypass brute-force protection by repeatedly restarting
the application between attempts.

## Decision

Persist rate limiter state to a dedicated SQLite table
(`rate_limiter_attempts`) through `RateLimiterRepository`.

Use a separate repository/connection strategy consistent with the
existing architecture to avoid unnecessary contention with the primary
database mutex.

The domain `RateLimiter` remains storage-agnostic and may operate with
either:

- in-memory storage (tests)
- SQLite-backed storage (production)

## Consequences

### Positive

- Brute-force protection survives application restarts.
- No external dependency added.
- Existing SQLite infrastructure is reused.

### Neutral

- Approximately one row upsert per failed login attempt.

### Negative

- Time-window behavior depends on system clock accuracy.
