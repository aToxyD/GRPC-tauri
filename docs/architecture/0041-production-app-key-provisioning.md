# ADR 0041: Production Application-Key Provisioning — GRPC_APP_KEY via Passphrase-Protected Store

# Status
Accepted (2026-08-08)

# Date
2026-08-08

# Owner
Architecture / Security

# Reference
- ADR-0039 (two-tier secret protection) — **amended** by this decision (adds a second
  `age::scrypt` site).
- Rule 38 (`scripts/check_arch.ts` §574-585) — **amended** by this decision.
- ARCHITECTURE_FREEZE.md §2.7 (two-tier secret protection) — **amended** by this decision.
- ADR-0038 (node identity & trust) — unchanged; the app key remains the root that
  protects node-managed secrets (`node_key_store`, backups, streaming encryption).
- `docs/runbooks/security-production-keys.md` — superseded where it conflicts with this
  decision (env-only provisioning).

# Context

Release builds of GRPC require `GRPC_APP_KEY` — an `age` x25519 identity — to boot.
Today it can only be supplied via the process environment:

- `resolve_app_encryption_key()` (`infrastructure/security/mod.rs`) returns
  `AppError::Configuration` when `GRPC_APP_KEY` is absent outside debug builds, and the
  startup path (`ConnectionFactory::new()`) fails closed before the first screen.
- The same key is the root that encrypts node-managed secrets (ADR-0039 §3): the node
  signing key (`node_key_store.rs`), streaming backup encryption, and local data
  encryption all derive from it via `AgeFileEncryptionProvider` (x25519).

Env-only provisioning has production gaps:

1. **No first-run onboarding.** A freshly installed release binary on a clean node
   cannot reach any UI to be provisioned; it exits at startup.
2. **Plaintext at rest in operator context.** The key sits in OS environment / shell
   history / process listings of every operator session.
3. **No custody model.** The operator cannot "own" the key as a portable secret; it is
   node-orchestration state, which conflicts with how `.adminkey` already treats
   operator-held secrets (ADR-0039).
4. **No verify/rotate/recovery story** independent of the environment.
5. **Operational asymmetry** with `.adminkey`: the ADMIN portable key already uses
   `age::scrypt` + passphrase; the app key — arguably the single most important secret on
   the node — has the weakest custody model of all.

The objective is to make the application key provisionable through the product itself,
with a clear, fail-closed lifecycle, while preserving the environment variable as an
explicit override for headless, CI, migration, and recovery paths.

# Decision

## 1. Application-key provisioning hierarchy (normative)

> Resolution order for the active app encryption key is **fixed**:
> **`GRPC_APP_KEY` env → unlocked store (`appkey.age`) → dev fallback (debug only) → fail-closed.**

| Rank | Source | Availability | Notes |
|------|--------|--------------|-------|
| 1 | `GRPC_APP_KEY` env | Always honored if present and valid | Explicit override: headless, CI, migration, recovery. NOT the primary onboarding path. |
| 2 | Unlocked `appkey.age` store (in-memory cache) | After successful `unlock_app_key` | Primary provisioning path for interactive nodes. |
| 3 | Embedded dev key (`DEV_AGE_KEY`) | `#[cfg(debug_assertions)]` only | `[DEV_SECURITY_WARNING]` log; unchanged behavior. |
| 4 | (none) | Release + store locked/missing + no env | `AppError::Configuration` — **locked** state; DB bootstrap is deferred, not aborted. |

- The hierarchy is evaluated by the **key resolution service**, not by scattered callers.
  `resolve_app_encryption_key()` is rewritten to consult env, then the cache.
- An explicit env value that fails validation (`AGE-SECRET-KEY-1` prefix) remains a
  `Validation` error and does **not** fall through to the store (rank 1 is terminal when
  present).

## 2. `appkey.age` store format

Portable-at-rest, JSON, `AppKeyFile { format_version, algorithm_version,
encrypted_identity }`:

- `encrypted_identity` — the raw `AGE-SECRET-KEY-1...` string bytes, encrypted with
  `age::scrypt(passphrase)`. The passphrase never leaves the backend.
- `format_version = 1` — file-schema evolution space (independent of algorithm versions,
  same convention as ADR-0039 §4).
- `algorithm_version = 1` — profile of the wrapped key (x25519 identity) in this file.
- Written **atomically** (write temp + rename) to `$XDG_DATA_HOME/GRPC/appkey.age`
  (reusing `GRPC_DATA_DIR = "GRPC"` from `adminkey_provider.rs`), file mode `0600`,
  `create_dir_all` as needed.

## 3. Rule 38 / Freeze §2.7 amendment (second scrypt site, deliberate)

`age::scrypt` becomes legal in **exactly two** files:

- `src-tauri/src/infrastructure/identity/adminkey_provider.rs` (portable operator
  key, ADR-0039) — unchanged.
- `src-tauri/src/infrastructure/security/appkey_store.rs` (passphrase-protected
  app-key-at-rest) — **new**, the sole provider of the store read/write.

`scripts/check_arch.ts` Rule 38 allow-list is updated to include `appkey_store.rs`.
ARCHITECTURE_FREEZE.md §2.7 and ADR-0039 are amended to state the two-site policy. This
is a deliberate contract amendment, not an `[arch:allow-*]` exception.

## 4. Lifecycle

| State | Trigger | Behavior |
|-------|---------|----------|
| `Unprovisioned` | No env, no `appkey.age`, no cached key | Release: boot reaches Security Setup UI. Commands: `get_security_status` → `provisioned=false`. |
| `Locked` | `appkey.age` present, not yet unlocked | Boot completes (DB deferred). `get_security_status` → `provisioned=true, unlocked=false`. Node access denied until unlock. |
| `Unlocked` | `unlock_app_key(passphrase)` success | Key cached in memory (`APP_KEY_CACHE: Mutex<Option<String>>`); DB bootstrap runs; `set_db` publishes state; login flow proceeds. |
| `UnlockFailed` | wrong passphrase / corrupt store | Fail-closed: rejected, stays locked, error surfaced. No fallback to env/dev. |
| `EnvOverride` | `GRPC_APP_KEY` set | Boot proceeds eagerly as today; store is ignored for key resolution. |

## 5. Deferred DB bootstrap

- When the key resolves from env (rank 1), startup is **eager** exactly as today.
- When the key must come from the store (rank 2), `ConnectionFactory::new()` is **not**
  called during boot. Instead `main.rs` extracts the DB-open sequence into a shared
  bootstrap function that runs on `unlock_app_key` success, then `AppState.set_db(...)`.
- `AgeFileEncryptionProvider` is already lazy (`get_identity()` resolves the key at
  encrypt/decrypt time), so constructing it while locked is safe.
- **No DB file is created while locked.** `get_db_path()` resolution stays eager for
  readiness/cleanup, but the SQLite connection and migrations do not start until unlock.

## 6. First-run key generation

- On first `initialize_app_key(passphrase)` (Security Setup), the node generates a fresh
  `age::x25519::Identity::generate()`, wraps the raw identity with the passphrase, writes
  `appkey.age` atomically, caches the key in memory, and returns the same "unlocked" state
  as `unlock_app_key`.
- Generation requires a strong passphrase (minimum length enforced at the boundary;
  policy mirrors `.adminkey`).

## 7. Optional offline portable export (`grpc-app-key.age`)

- On first `initialize_app_key`, the operator may **opt-in** to export the raw identity
  once, to a destination they choose, with an explicit warning that anyone with the
  exported key can decrypt node data. No automatic export, no persistence of the raw key,
  no inclusion in provisioning packages.
- This export is the **portable provisioning artifact** conventionally named
  `grpc-app-key.age`. It contains the **raw plaintext** `AGE-SECRET-KEY-1...` App Key:
  the `.age` extension does NOT mean the artifact is itself age-encrypted.
- `grpc-app-key.age` is **not** the node-local store `appkey.age` (§2) and is **not**
  subject to the store's `0600` filesystem invariant. It is an operator-controlled
  interchange artifact; its security boundary is operator custody and trusted offline
  transfer (§10.6), not file permissions.
- WILAYA is the parent node and operational source of the fleet App Key; UNIT nodes
  obtain the shared value from this artifact (currently via `GRPC_APP_KEY`, §10.3)
  rather than independently generating a divergent key.

## 8. Failure semantics (fail-closed)

| Scenario | Result |
|----------|--------|
| Wrong passphrase | `UnlockFailed` — rejected, stays locked |
| `appkey.age` missing + no env | `Unprovisioned` → Security Setup UI (release) |
| `appkey.age` corrupt / unparseable | rejected, locked; treated as operator action required |
| Store unlocked with a **different** key than the one that encrypted node material | `node_identity.key` / backups fail to decrypt → `Configuration`/`Internal` error → encrypted material unavailable (fail-closed) |

A wrong app key can never be silently swapped in; it surfaces as undecryptable node
material, which is the correct detection boundary.

## 9. Command surface

New IPC commands (registered in `commands/registry.rs`, thin auth-free handlers scoped to
pre-auth lifecycle):

- `get_security_status` → `{ provisioned, unlocked, store_path, source }`
- `initialize_app_key(passphrase, export_backup?)` → `AppKeyInitializeResult`
- `import_app_key(passphrase, artifact_path)` → `AppKeyImportResult` (fleet
  artifact import, APPKEY-003 Design B; ratified in §10.4)
- `unlock_app_key(passphrase)` → `AppKeyUnlockResult`
- `export_app_key_backup()` → guarded re-export path (requires unlocked state)

## 10. Interchange Encryption Key Scope (WILAYA ↔ UNIT bootstrap) — Model C

> Decision record for the previously open App Key interchange question
> (ADR-0044 §12 / A44-13): the current architecture resolves it as **Model C**
> — a shared App Key between WILAYA and its UNIT fleet. This section is the
> canonical scope definition.

### 10.1 Confidentiality root, not identity/signing credential

- The App Key (`age::x25519` identity) is an **encryption / confidentiality
  root only**. It carries no node identity, no authorization, and no signing
  authority.
- It MUST NOT be treated as an Ed25519 signing credential. Package authenticity
  is bound to node identity (`signature_version = 2`, RFC
  `2026-08-04-node-identity-trust`), the WILAYA trust anchor, and the Root
  chain — never to the App Key.
- The App Key and `GRPC_PACKAGE_SIGNING_KEY` remain unrelated secrets
  (ADR-0044 §12 / ADR-0045 §22.2): no derivation, no shared root.

### 10.2 Shared key requirement (bootstrap)

- `.unit` and all encrypted interchange use the **exporter's** App Key
  (`file_encryption.rs`, `AgeFileEncryptionProvider`), not a recipient-targeted
  key. There is no recipient-targeted encryption mechanism in the current
  architecture.
- Consequently, WILAYA ↔ UNIT bootstrap **requires the same App Key material**
  on the WILAYA node and the intended UNIT fleet: the shared value is what
  decrypts the `.unit` package (and every encrypted `.sync` / backup artifact).
- Recipient-targeted encryption (B5) remains a **future** architectural
  decision for post-bootstrap packages; it is NOT implemented here.

### 10.3 Approved deployment/bootstrap mechanism: `GRPC_APP_KEY`

- WILAYA is the **parent node** and the operational source of fleet security: it holds
  the fleet App Key and provisions it to UNIT child nodes. UNIT nodes do not
  independently generate a different App Key for `.unit` provisioning.
- The approved mechanism for supplying the shared value is the **`GRPC_APP_KEY`
  environment variable** (rank 1 in the resolution hierarchy, §1), set by the
  operator to the **same** valid `age` identity on the WILAYA node and every
  target UNIT at deployment/bootstrap time. As a project-native convenience,
  `import_app_key` (§9, §10.4) imports the same value from the portable
  artifact into a fresh local store — the store remains rank 2 in the
  resolution hierarchy.
- The operator-controlled value is typically the one-time **portable provisioning
  artifact** `grpc-app-key.age` (§7) exported from the WILAYA node — raw plaintext
  App-Key material, carried on the same trusted offline channel as the provisioning
  ceremony.
- The store (`appkey.age`) remains the primary path for interactive single-node
  onboarding (§1 rank 2); `GRPC_APP_KEY` is the fleet-deployment path.

### 10.4 Prohibited operations

- **Copying `appkey.age` between nodes is PROHIBITED.** The store is a
  passphrase-protected node-local artifact (§2); it is not an interchange
  vehicle. Interchange uses the approved `GRPC_APP_KEY` mechanism (§10.3) or
  the ratified `import_app_key` command (below).
- **`import_app_key` (APPKEY-003, Design B — ratified amendment)** is the
  project-native provisioning command for the fleet path: it reads the
  WILAYA-sourced portable artifact `grpc-app-key.age` (§7) on the backend
  (the renderer sends only the artifact path), validates it (extension
  `age`/`key`/`txt`, 64 KiB cap, single line, `AGE-SECRET-KEY-1` prefix),
  wraps the App Key with the operator passphrase into a **fresh** local
  `appkey.age` store via the standard `AppKeyStore` write path (scrypt +
  atomic write + `0600`), primes the runtime cache, and runs the deferred
  DB bootstrap (`finish_unlock`). Constraints:
  - Refuses in ALL cases when `appkey.age` already exists — existing stores
    are never overwritten, replaced, rotated, or deleted; the local
    generation path (`initialize_app_key`) and the import path are mutually
    exclusive per node.
  - The artifact is never modified, renamed, or deleted; no `0600` is applied
    to it (plaintext portable-artifact semantics, §10.4 artifact
    distinction); its raw value never crosses IPC, the DOM, or logs.
  - Import is confidentiality/decryption material provisioning only — it
    introduces no identity, trust, signing, or B8 semantics (no role
    detection, no UNIT-only authorization, no import for identity/certs/
    trust/`.unit`).
  - The `GRPC_APP_KEY` resolution rank (env rank 1, §1) is unchanged: while
    the env var is set, it wins over the imported store; an imported store is
    inert until then.
- **Artifact distinction (authoritative)**: `appkey.age` (node-local protected
  store) and `grpc-app-key.age` (portable plaintext provisioning/export artifact)
  are NOT the same kind of artifact. The portable artifact is intentionally
  plaintext App-Key material; its `.age` extension does not imply encryption, it
  is NOT a node-local credential store, and no mandatory `0600` filesystem
  invariant applies to it. The `0600` requirement applies to the local
  `appkey.age` store only and is unchanged.

### 10.5 Node binding (operational, not cryptographic)

- App Key equality **cannot be cryptographically node-bound**: the `age`
  x25519 identity carries no node identity, so the app cannot prove "this key
  belongs to this node". Enforcement is operational:
  - same operator-controlled value supplied consistently to WILAYA and the
    intended UNIT fleet (§10.3);
  - mismatched keys surface as undecryptable material (fail-closed, §8);
  - operator-custody rules (below).

### 10.6 Operational controls

- Operator custody: the shared key is operator-managed and operator-carried; it
  is never written into source, never committed to git, never logged by CI, and
  never included in provisioning packages (§7).
- The portable artifact `grpc-app-key.age` is **plaintext**: possession of the
  artifact exposes the fleet App Key. Its security depends on operator custody,
  trusted offline transfer, avoiding Git/CI/public/untrusted storage, limiting
  unnecessary copies, and deleting temporary copies when provisioning is
  complete where operationally appropriate. Filesystem mode is NOT the primary
  security boundary for this artifact; no `0600` invariant is imposed on it.
- The node-local `appkey.age` store keeps its protected handling: file mode
  `0600`, atomic writes (§2), secrets only via environment / protected secret
  storage.
- The same value must be supplied consistently to WILAYA and the intended UNIT
  fleet; divergent per-node values produce fail-closed decryption failures.

### 10.7 Scope exclusions

- App Key **rotation is outside the current scope** (as in §"Out of scope").
- Recipient-targeted encryption (B5) and any `.unit` / package format change are
  out of scope for this decision.
- The Ed25519 trust chain, the WILAYA trust anchor, and the B8 anchor-first
  gate are unchanged by this decision.

### 10.8 Packaged-identity exception — UNIT node secret transport (2026-08-15)

> Amendment synchronized with the ADR-0044 packaged-identity bootstrap and the
> RFC `2026-08-04-node-identity-trust` §3.12 D2 clarification (2026-08-15).

The general rule — *node signing keys never leave the node* — has one narrow,
explicit exception for the **packaged UNIT identity**:

- In the packaged `.unit` bootstrap flow the WILAYA generates a **UNIT** Ed25519
  keypair **in memory** at export time and embeds the signed UNIT certificate +
  32-byte secret inside the encrypted `.unit` package.
- The UNIT secret is transported **only** inside that encrypted artifact (App
  Key-encrypted, §10.2) and is installed on the UNIT node's own `NodeKeyStore`
  through a guarded, never-overwrite install. It is **never written to the
  WILAYA `NodeKeyStore`** — the WILAYA retains no copy.
- This does NOT change the WILAYA/UNIT node-key custody rule (ADR-0039 §3): the
  WILAYA's own node signing key and the UNIT's own node signing key remain
  x25519-protected node-managed secrets. `.adminkey` remains the only portable
  secret (`age::scrypt`).
- The App Key retains confidentiality-only status (§10.1) and is never a signing
  credential; the packaged UNIT certificate is Ed25519-authenticated via the
  WILAYA trust anchor (RFC §3.10), independent of the App Key.

# Consequences

- A clean release node reaches a UI on first run and can be provisioned in-product;
  `GRPC_APP_KEY` becomes an optional override instead of a hard prerequisite.
- DB bootstrap is deferred until unlock when using the store; startup stays eager when
  env is used.
- `age::scrypt` has exactly two legal sites (`.adminkey`, `appkey.age`), both
  operator-passphrase custody; node-managed secrets remain x25519-only.
- Node data remains bound to the node's app key: losing the passphrase + backup = data
  loss, matching the existing `.adminkey` custody model and the runbook's warning.
- `validate_production_security_environment()` is adjusted: `GRPC_APP_KEY` is no longer
  required when a store exists, but the store must be unlockable before node access.

# Out of scope

- `GRPC_PACKAGE_SIGNING_KEY` provisioning — stays env-only, lazy-resolved, fleet secret.
- Key rotation (ADR-0006 scope) — this ADR provisions the first key only.
- Changing `AgeFileEncryptionProvider` encryption semantics (x25519 unchanged).
- Any change to `grpc-licensing` (addressed separately by ADR-0042).

# Implementation

- B3 (security): `appkey_store.rs` (scrypt store), key-resolution service + cache in
  `security/mod.rs`, amended `resolve_app_encryption_key()`, deferred bootstrap in
  `main.rs`.
- B2 (commands): `commands/security.rs` + registry registration.
- B1 (frontend): `security.contract.ts`, Security Setup / Unlock page, routing.
- Rule 38 / Freeze §2.7 / ADR-0039 amendment; runbook updates.
