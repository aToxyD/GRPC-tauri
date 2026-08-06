-- =============================================================================
-- GRPC SQLite Schema Migration 004 — Sync transport ordering ledger
-- RFC 2026-08-04-node-identity-trust §3.4.1 / ADR-0038 / B4
-- Version: 4
--
-- Additive-only. Does not alter any pre-existing column, index, or row.
--
-- B4 introduces per-issuer transport ordering for node-identity-signed packages
-- (signature_version = 2). The two new columns on `applied_sync_packages` record
-- the transport metadata of every applied package (audit trace), and the
-- `sync_issuer_sequence` ledger stores the last applied sequence per issuing
-- node so the Transport Guard can be evaluated fail-closed on import:
--
--   expected = last_applied_sequence[issuer] + 1
--   if incoming.sequence != expected → reject / defer
--
-- The guard itself is pure (domain / sync_integrity); this table is the only
-- persistence it needs. Legacy HMAC/V1 packages (sequence NULL) are unaffected
-- during the deprecation window.
-- =============================================================================

ALTER TABLE applied_sync_packages ADD COLUMN package_sequence INTEGER;
ALTER TABLE applied_sync_packages ADD COLUMN issuer_identity_id TEXT;

CREATE TABLE IF NOT EXISTS sync_issuer_sequence (
    issuer_identity_id TEXT PRIMARY KEY,
    last_applied_sequence INTEGER NOT NULL,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);
