# Release Certification Checklist

Every release must pass all gates below before certification.

---

## Pre-Release Gates

### 1. Architecture Audit

```text
bun scripts/check_arch.ts
Expected: 0 errors, 0 warnings
Status:   [x] ✅
```

### 2. Governance Audit

```text
bun scripts/check_arch.ts — GROUP 27/28 rules
FE-158: Governance drift — 0 errors
FE-159: Contract mutation — 0 errors
FE-165: Release gate — 0 errors
FE-166: Snapshot approval — 0 errors
FE-167: Certification consistency — 0 warnings
Status:   [x] ✅
```

### 3. Projection Audit

```text
FE-160: Projection mutation — 0 warnings
FE-157: Projection surface — 0 errors
Status:   [x] ✅
```

### 4. Contract Audit

```text
FE-153: Contract ownership — 0 errors
FE-159: Contract mutation — 0 errors
FE-156: Contract size — 0 warnings
Status:   [x] ✅
```

### 5. Import Graph Audit

```text
FE-152: Projection ownership — 0 errors
FE-154: Barrel integrity — 0 errors
FE-155: Architecture drift — 0 warnings
Status:   [x] ✅
```

### 6. Snapshot Verification

```text
FE-158: Snapshot drift — 0 errors
FE-166: Snapshot approval — 0 errors
Status:   [x] ✅
```

### 7. Test Suite

```text
npm test
Expected: all tests pass (16/16 files, 86/86 tests)
Status:   [x] ✅
```

### 8. Build

```text
npm run build
Expected: clean build (0 errors)
Status:   [x] ✅
```

### 9. Backend Compilation

```text
cargo check
Expected: clean (0 errors)
Status:   [x] ✅
```

---

## Certification Gates Summary

| Gate | Command | Expected | Status |
|------|---------|----------|--------|
| Architecture Audit | `bun scripts/check_arch.ts` | 0 errors, 0 warnings | [x] ✅ |
| Projection Audit | FE-157, FE-160 | 0 violations | [x] ✅ |
| Contract Audit | FE-153, FE-156, FE-159 | 0 violations | [x] ✅ |
| Import Graph | FE-152, FE-154, FE-155 | 0 violations | [x] ✅ |
| Snapshot Verification | FE-158, FE-166 | 0 violations | [x] ✅ |
| Test Suite | `npm test` | 86/86 pass | [x] ✅ |
| Build | `npm run build` | Clean | [x] ✅ |
| Cargo Check | `cargo check` | Clean | [x] ✅ |

---

## Release Sign-off

```text
Release Version:      v1.2.0
Certification Version: v6
Certification Date:   2026-06-01
Snapshot Version:     v5-freeze
Approver:             governance-certification
```
