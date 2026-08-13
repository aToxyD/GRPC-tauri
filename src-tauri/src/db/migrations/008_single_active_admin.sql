-- =============================================================================
-- GRPC SQLite Schema Migration 008 — Single ACTIVE ADMIN invariant (SEC-002)
-- Version: 8
--
-- Additive-only. Enforces at the DATABASE level that the Identity Store can
-- hold at most one ACTIVE ADMIN credential. The service-level one-time gate in
-- `issue_first_admin_key` (SEC-002) is the operational guard; this index is the
-- hard invariant that survives any future insertion path (import, sync,
-- test-only helpers) that mints an ADMIN credential.
--
-- Recovery (SEC-002-09): the ceremony may SUPERSEDE an unusable ACTIVE ADMIN
-- credential in the same transaction; because the supersede statement runs
-- before the insert, only one ACTIVE ADMIN row exists at commit time, so the
-- index never conflicts.
-- =============================================================================

CREATE UNIQUE INDEX IF NOT EXISTS idx_identity_single_active_admin
    ON identity_store(subject_type)
    WHERE subject_type = 'ADMIN' AND status = 'ACTIVE' AND deleted = 0;
