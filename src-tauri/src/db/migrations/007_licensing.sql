-- =============================================================================
-- GRPC SQLite Schema Migration 007 — Licensing consumer derived view
-- ADR-0042 (Licensing Consumer Integration) / artifact-spec.md v1 / Phase B
-- Version: 7
--
-- Additive-only. Does not alter any pre-existing column, index, or row.
--
-- `grpc` is the CONSUMER of Signed Licensing Artifacts produced by the
-- Licensing Authority (`grpc-licensing`, ADR-0002..0006). This migration adds
-- the consumer's DERIVED, replaceable view of license state — never
-- authoritative (ADR-0003 Invariant 2): every observable transition
-- (Active → Suspended/Revoked/Expired) arrives as a NEWLY ISSUED signed
-- artifact; `grpc` performs NO local mutation of license state.
--
-- Three tables:
--
--   1. `licensing_anchor` — the single active licensing Trust Anchor
--      (provisioning-v1, artifact-spec §9). Exactly ONE active anchor per
--      installation (ADR-0005 Invariant 6); enforced by a partial unique
--      index. Importing a newer package REPLACES the active anchor (rotation);
--      a second simultaneous active anchor is rejected at the storage layer.
--      Anchor presence is the licensing enforcement gate trigger: the gate is
--      dormant until an active anchor is installed, then globally active for
--      every non-exempt action (ADR-0042 §5). Dormant state is derived from
--      this table — never a memory flag — so activation is persistent across
--      restarts.
--
--   2. `licensing_license` — per-license derived view keyed on `license_id`.
--      Import REPLACES any previously held row for that id (full overwrite,
--      never a merge; ADR-0003 Invariant 2). `artifact_json` stores the exact
--      canonical envelope bytes received, so `verify_license()` and the
--      enforcement gate can re-run the FULL verification pipeline (§4 steps
--      1–6) deterministically against the current anchor and node key.
--
--   3. `licensing_events` — append-only audit of import/re-verification
--      outcomes (P4): `verified` / `rejected` / `not-for-this-node`.
--
-- Subject binding (ADR-0042 §4): `payload.subject.id` is the opaque
-- base64url(URL_SAFE_NO_PAD) encoding of the current node's raw Ed25519 public
-- key (32 bytes). No certificate, no trust-chain, no cert status participates
-- in binding — the raw node key material (`NodeKeyStore`) only.
-- =============================================================================

CREATE TABLE IF NOT EXISTS licensing_anchor (
    key_id       TEXT PRIMARY KEY,
    public_key   BLOB NOT NULL,                -- raw 32-byte Ed25519 public key
    algorithm    TEXT NOT NULL DEFAULT 'Ed25519',
    installed_at TEXT NOT NULL,
    is_active    INTEGER NOT NULL DEFAULT 0 CHECK (is_active IN (0, 1))
);

-- Single active anchor invariant (ADR-0005 Invariant 6) at the storage layer.
CREATE UNIQUE INDEX IF NOT EXISTS ux_licensing_anchor_single_active
    ON licensing_anchor (is_active)
    WHERE is_active = 1;

CREATE TABLE IF NOT EXISTS licensing_license (
    license_id             TEXT PRIMARY KEY,
    artifact_id            TEXT NOT NULL,
    type_key               TEXT NOT NULL,
    subject_id             TEXT NOT NULL,
    entitlements_json      TEXT NOT NULL,      -- JSON array of entitlement keys
    status                 TEXT NOT NULL,
    contract_version_major INTEGER NOT NULL,
    contract_version_minor INTEGER NOT NULL,
    artifact_json          TEXT NOT NULL,      -- exact canonical envelope bytes for re-verification
    imported_at            TEXT NOT NULL,
    last_verified_at       TEXT
);

CREATE INDEX IF NOT EXISTS idx_licensing_license_status
    ON licensing_license (status);

CREATE TABLE IF NOT EXISTS licensing_events (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    event_type  TEXT NOT NULL,                 -- import | re-verify | dry-run
    outcome     TEXT NOT NULL,                 -- verified | rejected | not-for-this-node
    license_id  TEXT,
    artifact_id TEXT,
    key_id      TEXT,
    message     TEXT,
    recorded_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_licensing_events_recorded_at
    ON licensing_events (recorded_at DESC);
