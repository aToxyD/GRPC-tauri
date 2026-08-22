-- =============================================================================
-- GRPC SQLite Schema Migration 010 — admin_access issuer-only producer stream
-- ADR-0051 (Admin-Only B8 Account Synchronization — Accepted 2026-08-22)
-- Version: 10
--
-- Additive-only. Does not alter any pre-existing column, index, or row.
--
-- ADR-0051 §7: `admin_access` exports allocate their transport sequence from
-- a dedicated stream keyed by issuer_identity_id ALONE (no target dimension):
-- the package is fleet-wide WILAYA → all UNIT nodes with NO target binding,
-- so a single per-issuer stream serves every UNIT. The same signed artifact
-- is independently importable by every authorized UNIT against its own local
-- consumer ledger (`sync_issuer_sequence`) — replay/ordering state stays
-- strictly local, so WILAYA sequence N is accepted by UNIT-A and UNIT-B alike.
--
-- The stream is deliberately decoupled from the global per-issuer producer
-- ledger (`sync_issuer_sequence_state`, migration 006), the per-target
-- `identity_access` stream (migration 009), and every other kind's numbering.
--
-- Allocation model (identical advance-on-success contract to migrations
-- 006/009):
--   read last_issued_sequence[issuer] → next = last + 1
--   build/sign the package file with `next`
--   commit `next` ONLY after a successful export (retry reuses the number)
--
-- Concurrency: race-safe under the single-writer SQLite mutex
-- (Mutex<Option<Connection>>) which is held by the exporting command across
-- the whole begin→build→commit span.
-- =============================================================================

CREATE TABLE IF NOT EXISTS admin_access_export_sequence (
    issuer_identity_id TEXT PRIMARY KEY,
    last_issued_sequence INTEGER NOT NULL,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);
