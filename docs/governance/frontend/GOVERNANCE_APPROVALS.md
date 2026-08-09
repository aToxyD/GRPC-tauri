# Governance Approvals Registry

Tracks architecture-affecting decisions and governance approvals for the frontend layer.

## Format

| Date | Change | Rule Impact | Approver | Snapshot Version |
|------|--------|-------------|----------|-----------------|
| YYYY-MM-DD | Description of change | Rules affected | Who approved | Snapshot ref |

---

## Approvals

| Date | Change | Rule Impact | Approver | Snapshot Version |
|------|--------|-------------|----------|-----------------|
| 2026-06-01 | Phase 4A certification baseline — domain isolation, contract ownership, projection purity | FE-152, FE-153, FE-154, FE-155, FE-156, FE-157 | governance-certification | v4A-baseline |
| 2026-06-01 | Phase 4B certification baseline — continuous governance, snapshot enforcement, suppression lifecycle | FE-158, FE-159, FE-160, FE-162, FE-163 | governance-certification | v4B-baseline |
| 2026-06-01 | Initial governance snapshots generated for contract, domain, projection, and import graph | FE-158 baseline | governance-certification | v4B-baseline |
| 2026-06-01 | All existing suppressions annotated with metadata (Reason, Date, Owner) | FE-162 compliance | governance-certification | v4B-baseline |
| 2026-06-01 | Phase 5 — Governance freeze and release certification | FE-165, FE-166, FE-167 | governance-certification | v5-freeze |
| 2026-06-01 | Governance snapshots frozen at v5-freeze baseline | FE-158, FE-166 | governance-certification | v5-freeze |
| 2026-06-01 | v1.2.0 release certification — 6 atomic commits, all 9 release gates pass | FE-165, FE-166, FE-167 | governance-certification | v5-freeze |
| 2026-06-01 | Governance v2 — 47 micro-rules replaced by 5 invariants (invariant-based governance) | All FE rules (FE-100–FE-167) | governance-certification | v5-freeze |
| 2026-08-06 | B7 credential rotation — 5 post-auth identity contract exports (`beginWilayaRotation`, `finalizeWilayaRotation`, `beginUnitRotation`, `signUnitRotationRequest`, `finalizeUnitRotation`) added to `identity.contract.ts`; `contracts.snapshot.json` identity entry recertified (10→15) | FE-158 (contract snapshot) | governance-certification | v5-freeze |
| 2026-08-07 | B8 (②) identity & access sync — 5 exports added to `sync.contract.ts` (`setFleetAdminPassword`, `setUnitUserPassword`, `setAccountStatus`, `exportIdentityAccessPackage`, `importIdentityAccessPackage`); `IdentityAccessImportResult` projection added to `types.ts`; `contracts.snapshot.json` sync entry recertified (10→15) and `projection-ownership.snapshot.json` recertified (102→103) | FE-158, FE-160 | governance-certification | v5-freeze |
| 2026-08-08 | B9 — app-key security contract: new `security.contract.ts` (4 exports: `getSecurityStatus`, `initializeAppKey`, `unlockAppKey`, `exportAppKeyBackup`), new `AppSecurityPage.svelte` registered at `/security`, `security` domain added to registry, `getSecurityStatus` approved as cross-domain read for `user`/`metrics`, `login` approved as cross-domain navigation for `security`; `contracts.snapshot.json` + `domain-ownership.snapshot.json` recertified | FE-152, FE-155, FE-158, FE-121 | governance-certification | v5-freeze |

---

## Governance Approval Policy

1. Any change that alters the governance baseline (contract ownership, projection ownership, domain ownership, import graph) **requires** a new approval entry.
2. Suppressions are valid for **90 days maximum**. Expired suppressions must be reviewed and renewed with a new approval entry.
3. Snapshot updates must be accompanied by a governance approval entry recorded here.
4. All approvals require documented justification and a named approver.
