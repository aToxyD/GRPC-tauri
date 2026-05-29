# Release Certification Process — GRPC-Tauri

## Overview

Every production release must pass the Release Certification Suite before deployment. This document defines the certification process, the gates, and the responsibilities.

## Certification Command

```bash
bun run scripts/release_certification.ts
```

This runs all gates and produces a PASS/FAIL summary with a machine-readable JSON report in `target/release_certification_<timestamp>.json`.

## Certification Gates

| Gate | What it verifies | Failure impact |
|------|------------------|----------------|
| **1. cargo test** | All unit and integration tests pass | Blocking — release cannot proceed |
| **2. cargo clippy** | Zero clippy warnings at `-D warnings` | Blocking |
| **3. cargo fmt** | Code formatting is consistent | Blocking |
| **4. check:arch** | All 122+ architecture rules pass | Blocking |
| **5. Determinism suite** | Report reproducibility + recovery determinism | Blocking |
| **6. Exception governance** | No undocumented `[arch:allow-*]` tags | Blocking |
| **7. Exception expiry** | No expired architectural exceptions | Blocking |

## Pre-Certification Checklist

Before running certification:

1. [ ] All code changes committed and reviewed
2. [ ] ADR exception registry is up-to-date
3. [ ] All `[arch:allow-*]` tags reference a valid ADR
4. [ ] No expired architectural exceptions
5. [ ] Production readiness checklist is current

## Failed Gate Recovery

If a certification gate fails:

1. **Fix the root cause** — do not bypass the gate
2. **Re-run the certification** from scratch
3. **Document the fix** in the release notes

## Release Artifacts

| Artifact | Location | Format |
|----------|----------|--------|
| Certification report | `target/release_certification_<ts>.json` | JSON |
| Build artifacts | `src-tauri/target/release/` | Binaries |

## Sign-Off Requirements

| Role | Responsibility |
|------|---------------|
| Lead Architect | Verify architecture audit + ADR governance |
| QA Lead | Verify all tests pass |
| Security Officer | Verify no security regressions |
| Project Owner | Final sign-off |

## Emergency Override

In exceptional circumstances, the Lead Architect may waive a non-critical gate with documented rationale. The override must be:

1. Documented in the release notes
2. Time-bound (max 7 days)
3. Approved by the Project Owner
