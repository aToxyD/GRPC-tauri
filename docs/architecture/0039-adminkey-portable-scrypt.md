# ADR 0039: Two-Tier Secret Protection — age::x25519 for Node Secrets, age::scrypt for Portable `.adminkey`

# Status
Accepted (2026-08-04)

# Date
2026-08-04

# Owner
Architecture / Security

# Reference
- RFC `docs/architecture/rfcs/2026-08-04-node-identity-trust.md` §3.8 (`.adminkey`), §3.10.
- ADR-0038 §5 (Challenge–Response authentication).
- Rule 38 (`scripts/check_arch.ts`) — amended by this decision.
- ARCHITECTURE_FREEZE.md §2.7 — amended by this decision.
- ADR-0019 (legacy crypto isolation) — unchanged; this decision refines the allowed crypto
  surface, it does not weaken isolation.

# Context

The identity model distinguishes two secret classes with different lifecycles and custody
models (RFC §3.8, ADR-0038 §5):

1. **Node signing keys (WILAYA / UNIT)** — owned by the node, never leave the node,
   protected by an encryption key derived from `GRPC_APP_KEY`. These are node-scoped and
   must remain bound to the node.
2. **Operator key material (`.adminkey`)** — represents the ADMIN identity, must be
   **portable** between devices, self-contained, and unlocked by a human-memorable operator
   passphrase. It must depend on no node-local secret (`GRPC_APP_KEY`, node identity, or
   any local secret).

The previous contract (Rule 38 / Freeze §2.7) banned `age::scrypt` in favor of
`age::x25519` exclusively. `age` exposes only two recipient types: x25519 (public-key,
node-bound) and scrypt (passphrase). Forcing `.adminkey` under x25519 makes it node-locked
and defeats the RFC's portability requirement; reimplementing a KDF+AEAD outside `age`
would duplicate what `age::scrypt` already provides.

# Decision

## 1. Two-tier secret protection (normative)

> `age::x25519` is mandatory for **node-managed secrets**.
> `age::scrypt` is permitted **exclusively** for portable operator key material (`.adminkey`).

| Secret class | Custody | Encryption |
|--------------|---------|------------|
| Node signing keys (WILAYA/UNIT) — `node_key_store` | On-node, never leaves owner | `age::x25519` with key derived from `GRPC_APP_KEY` |
| Portable operator key — `.adminkey` | Operator-held, portable between devices | `age::scrypt` with strong operator passphrase |

## 2. Rule 38 amendment (deliberate, not an exception)

`scripts/check_arch.ts` Rule 38 is rewritten:

- `age::scrypt` is an **error** in `src-tauri/src/**/*.rs` everywhere **except**
  `src-tauri/src/infrastructure/identity/adminkey_provider.rs` (the sole portable-key
  provider). Comments are excluded as before.
- The error message states the two-tier policy.

This is a deliberate contract amendment approved through the RFC-to-ADR process, not a
time-limited `[arch:allow-*]` exception.

## 3. `.adminkey` file format

Self-contained, JSON, `AdminKeyFile { format_version, algorithm_version, certificate,
encrypted_private_key }`:

- `encrypted_private_key` — the Ed25519 secret key, age-encrypted with
  `age::scrypt(passphrase)`. The passphrase never leaves the backend.
- `certificate` — the current ADMIN `IdentityCertificate` (includes the issuer signature).
- `credential_id` / `generation` are read from the embedded certificate (single source of
  truth, P2); they are not duplicated as top-level fields.

## 4. Independent version identifier spaces

The following are **three independent identifier spaces**. Their values coincide today
(`format_version=1`, `algorithm_version=2`, `signature_version=2`) by regulatory
convention, **not** by semantic relationship.

| Identifier | Space it identifies | Value today |
|-----------|---------------------|-------------|
| `format_version` | `.adminkey` file schema evolution | `1` |
| `algorithm_version` | Identity algorithm profile (`IDENTITY_ALGORITHM_PROFILE_ED25519`) | `2` |
| `signature_version` | Certificate/package signing scheme (`SIGNATURE_VERSION_ED25519`) | `2` |

- `assert_eq!(algorithm_version, signature_version)` is a **defect**; no code may assume a
  permanent relationship between the two spaces.
- `format_version` bumps on file-layout changes; `algorithm_version` bumps on identity
  profile changes (e.g. a future Ed448 profile or a new canonical encoding); `signature_version`
  bumps on signing-scheme changes. Each evolves independently.

## 5. Canonical encoding contract

- `IdentityCertificate::canonical_bytes()` and `ChallengeMessage::canonical_bytes()` are
  the **single source of truth** for the signed representation.
- `CANONICAL_ENCODING_VERSION = 1` is emitted as the first two bytes of both encodings.
- Issuance, verification, fingerprinting (future), and tests all call the same function.
  No code may build the signed byte layout by hand.
- Every object that can be issued MUST be verifiable through the exact inverse path
  (**Verification mirrors Issuance**):
  `issue → canonical_bytes() → sign_certificate()` / `verify_certificate() → canonical_bytes()`.
  `verify(raw bytes)` is prohibited.
- Low-level `sign(&[u8])` / `verify(&[u8], ...)` remain internal to the infrastructure
  layer; domain and application layers only use the entity-level default methods
  (`sign_certificate`, `sign_challenge`, `verify_certificate`, `verify_challenge`).

## 6. Certificate signature lifecycle

- `IdentityCertificate.signature: Option<Ed25519CertificateSignature>` where
  `Ed25519CertificateSignature([u8; 64])` (fixed 64-byte Ed25519 signature).
- `Option` is a **migration mechanism only**: legacy records may carry `None` during the
  deprecation window. After Identity Trust activation, every new certificate MUST carry a
  signature; a certificate without one is a programming error (`require_signed()`).
- Storage keeps the signature as an opaque `Vec<u8>`/BLOB; the domain enforces the fixed
  length at the boundary.

# Consequences

- `.adminkey` is portable and self-contained: it depends on no node secret, no network,
  no Root, no WILAYA key to decrypt.
- Node signing keys remain node-bound and protected by the app encryption key.
- The separation between **node identity** and **operator identity** custody is preserved
  per RFC §3.8.
- A follow-up additive migration may set the `signature` column to NOT NULL when V1
  support is removed.

# Out of scope

- Changing the trust chain, credential lifecycle, or authorization/session model.
- Allowing `age::scrypt` for any other purpose (backups, sync packages, node keys, logs).
- X.509 / PKI.

# Implementation

- B3: two-tier crypto enforcement (this ADR), `Ed25519CertificateSignature`,
  `canonical_bytes()` on certificate + challenge, `.adminkey` provider, node key store,
  Challenge–Response.
