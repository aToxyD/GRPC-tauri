-- =============================================================================
-- GRPC SQLite Schema Migration 011 — unified per-target transport sequence
-- ADR-0053 (Unified Per-Target Transport Sequence — Accepted 2026-08-24)
-- Version: 11
--
-- Producer-side unification (SEC-030/031 → SEC-032). The consumer
-- TransportGuard scope is ONE contiguous per-issuer stream per receiving node
-- (`sync_issuer_sequence`, migration 004 — UNTOUCHED, byte-frozen), while the
-- producer had THREE fragmented allocation scopes:
--   - sync_issuer_sequence_state          (migration 006 — global per-issuer)
--   - identity_access_export_sequence     (migration 009 — per-target, one kind)
--   - admin_access_export_sequence        (migration 010 — issuer-only, one kind)
-- ADR-0053 replaces all three with ONE canonical producer allocator keyed by
-- (issuer_identity_id, target_node_id) across ALL pipeline-imported kinds,
-- making the producer scope structurally identical to the frozen consumer
-- scope (each receiving node's ledger implicitly represents target = self).
--
-- Canonical target resolution (ADR-0053 §3.3):
--   UNIT recipient   → units.code           (authoritative row-validated)
--   WILAYA recipient → settings.wilaya_code
-- Never: unit_name / renderer-provided arbitrary IDs / issuer_identity_id as
-- target / source_node_id as target.
--
-- Allocation model (identical advance-on-success contract to 006/009/010):
--   read last_issued_sequence[(issuer, target)] → next = last + 1 (first = 1)
--   build/sign/write the package file with `next`
--   commit `next` ONLY after a successful export (retry reuses the number;
--   a failed export burns NO sequence; no gaps).
--
-- Migration policy (ADR-0053 §8): pre-release controlled development reset.
-- All pre-SEC-031 sync artifacts are VOID and must be regenerated. The
-- superseded producer allocator tables are retired here; consumer tables
-- (`sync_issuer_sequence`, `sync_applied_packages`) are NOT touched.
--
-- Concurrency: race-safe under the single-writer SQLite mutex
-- (Mutex<Option<Connection>>) which is held by the exporting command across
-- the whole begin→build→commit span.
-- =============================================================================

CREATE TABLE IF NOT EXISTS transport_export_sequence (
    issuer_identity_id TEXT NOT NULL,
    target_node_id TEXT NOT NULL,
    last_issued_sequence INTEGER NOT NULL CHECK (last_issued_sequence >= 1),
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (issuer_identity_id, target_node_id)
);

DROP TABLE IF EXISTS sync_issuer_sequence_state;
DROP TABLE IF EXISTS identity_access_export_sequence;
DROP TABLE IF EXISTS admin_access_export_sequence;
