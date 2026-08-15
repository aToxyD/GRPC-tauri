-- =============================================================================
-- GRPC SQLite Schema Migration 009 — identity_access per-target producer stream
-- RFC 2026-08-04-node-identity-trust §3.4.1 (amended 2026-08-15) / ADR-0045 §26.9
-- (F-1 Option A) / B8 (ADR-0045)
-- Version: 9
--
-- Additive-only. Does not alter any pre-existing column, index, or row.
--
-- F-1 Option A (owner-ratified 2026-08-15): `identity_access` exports allocate
-- their transport sequence from a stream scoped by
-- (issuer_identity_id, target_unit_code) instead of the global per-issuer
-- producer ledger (`sync_issuer_sequence_state`, migration 006). This makes
-- the producer compatible with the ratified consumer Transport Guard rule
-- "first import from an issuer on an EMPTY ledger MUST be sequence 1"
-- (ADR-0045 A45-06 / B8 control 11) for EVERY fresh target UNIT: each unit
-- receives its own 1, 2, 3, ... stream from the same WILAYA issuer.
--
-- The other V2 kinds (products, daily_report, monthly_summary,
-- stock_movements) keep the global per-issuer producer ledger unchanged.
-- `.unit` bootstrap artifacts keep fixed sequence 1 (ADR-0044 A44-08) and
-- never advance any ledger.
--
-- Allocation model (identical advance-on-success contract to migration 006):
--   read last_issued_sequence[(issuer, target)] → next = last + 1
--   build/sign the package file with `next`
--   commit `next` ONLY after a successful export (retry reuses the number)
--
-- Concurrency: race-safe under the single-writer SQLite mutex
-- (Mutex<Option<Connection>>) which is held by the exporting command across
-- the whole begin→build→commit span — the same guarantee the global producer
-- ledger relies on.
-- =============================================================================

CREATE TABLE IF NOT EXISTS identity_access_export_sequence (
    issuer_identity_id TEXT NOT NULL,
    target_unit_code TEXT NOT NULL,
    last_issued_sequence INTEGER NOT NULL,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (issuer_identity_id, target_unit_code)
);
