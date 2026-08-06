-- =============================================================================
-- GRPC SQLite Schema Migration 006 — Producer sequence state ledger
-- RFC 2026-08-04-node-identity-trust §3.4.1 / ADR-0038 / B6-B (Commit ④)
-- Version: 6
--
-- Additive-only. Does not alter any pre-existing column, index, or row.
--
-- B4 introduced the CONSUMER transport ledger (`sync_issuer_sequence`): the
-- last APPLIED sequence per issuing node identity, read by the Transport Guard
-- on import. B6-B (Commit ④) adds the PRODUCER mirror: the last ISSUED
-- sequence per LOCAL node identity, so the four V2 export paths allocate
-- monotonic, per-issuer package sequences with an "advance-on-success" model:
--
--   read last_issued_sequence[local_identity] → next = last + 1
--   build/sign the package file with `next`
--   commit `next` ONLY after a successful export (retry reuses the number)
--
-- The ledger is keyed on `issuer_identity_id` (the stable node identity) and
-- NEVER on `credential_id` — Transport ordering is independent of credential
-- generation (RFC §3.4.4), so continuity survives rotation/re-issue.
-- =============================================================================

CREATE TABLE IF NOT EXISTS sync_issuer_sequence_state (
    issuer_identity_id TEXT PRIMARY KEY,
    last_issued_sequence INTEGER NOT NULL,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);
