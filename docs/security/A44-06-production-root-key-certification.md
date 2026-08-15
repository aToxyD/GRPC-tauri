# سجل اعتماد مفتاح جذر الإنتاج (A44-06 Production Root-Key Certification)

- **Certification ID**: A44-06-2026-08-15
- **Date (ceremony)**: 2026-08-14 05:11 (local) — artifact timestamps
- **Date (certification record)**: 2026-08-15
- **Purpose**: Production Authority Root public verification key (offline bootstrap trust anchor)
- **Algorithm**: Ed25519 (RFC 8032) — 32-byte public key, Base64 (STANDARD) encoding

---

## 1. Certified Public Key

| Field | Value |
|-------|-------|
| Public key Base64 | `FTMc0JHAEL0EHqzB408Aqlx8NP6FjvvbVLC5JRatd0Q=` |
| Public key hex | `15331cd091c010bd041eacc1e34f00aa5c7c34fe858efbdb54b0b92516ad7744` |
| SHA-256 fingerprint (raw 32-byte public key) | `7b38f2e1584353f9a2bf4fcc578345db3660208fecfd4c042d4efe6270072f20` |
| SHA-256 fingerprint (Base64 string bytes) | `f9302a7e8a5970a196c6d68b50c8010f1d5c150457c07df955b4d445fdb68d08` |
| Length | 32 bytes (Ed25519) |
| Encoding | Base64 STANDARD (44 chars) / hex (64 chars) |

## 2. Ceremony Provenance

- **Tool**: `src-tauri/src/bin/root-signer.rs` — `init` subcommand (OsRng entropy, owner-only 0600 secret, refusal of pre-existing destinations, pair round-trip + sign round-trip self-verification before any write).
- **Artifacts (outside the repository, on the operator's machine)**:
  - `/home/atoxyd/Desktop/root-secret.hex` — secret seed, 64 hex chars + newline, permissions `0600`, owner `atoxyd`. **Never enters the repository.**
  - `/home/atoxyd/Desktop/root-public.key` — public key, 44 Base64 chars + newline.
- **Artifact timestamps**: 2026-08-14 05:11 (both files).
- **Pin commit**: `930a2a8` («security: pin production Authority Root public key», 2026-08-14 06:13) — embeds exactly the certified public key one hour after the ceremony artifacts.
- **Operator / approver**: atoxyd (repository owner; same identity as the gate-authorizing owner).

## 3. Independent Verification

Performed 2026-08-15 by the A44-06 certification gate (independent of the repository):

1. The public key file content equals the embedded pin: Base64 and hex byte-exact.
2. The secret seed in `root-secret.hex` independently re-derives to the exact same public key (derivation performed in memory; secret never displayed, printed, logged, or stored in the repository).
3. The keypair is self-consistent: secret ⇄ public-key file.
4. The embedded pin decodes to exactly 32 bytes (Ed25519).
5. The key is NOT any RFC 8032 §7.1 test vector (TEST-1/2/3) and is not derivable from naive/weak seeds (zero, one, 0x42, 0xff, sequential, ASCII candidates all checked — no match).
6. SHA-256 fingerprints recorded above.

## 4. Custody Statement

- The production root **private key** is NOT stored in the repository, has never entered git history, and is not present in source, tests, fixtures, logs, or release artifacts.
- The secret artifact lives on the operator's machine (`0600`, outside the repo), consistent with the offline root-signer procedure (root-signer.rs: «The Root private key NEVER lives in this repository»).
- Only the public key and its fingerprints may be recorded in repository documentation.

## 5. Embedded-Source Match

- `root_public_key.rs` → `PROD_ROOT_PUBLIC_KEY` equals exactly the certified public key above (Base64 and hex byte-exact). No replacement occurred.

## 6. Certification Status

**CERTIFIED** — A44-06 satisfied. The embedded `PROD_ROOT_PUBLIC_KEY` is the certified Production Authority Root public key.

### ملاحظة تاريخية (Historical Note)

وثائق سابقة (ADR-0044 §9.4، بوابات §28.x، ADR-0045 §25/§26.1/§26.6/§26.8، RFC §3.6/B5، تعليق `root_public_key.rs`) وصفت المثبّت بأنه ناقل RFC 8032 TEST-2 ومحجوب. هذا السجل يلغي ذلك: المراسم نُفِّذت في 2026-08-14 والاعتماد استُكمل في 2026-08-15.
