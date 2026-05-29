-- Migration 004: Audit Schema Evolution
-- Adds structured columns for governance-grade audit queries
-- while preserving full backward compatibility with existing audit_log rows.
--
-- See docs/architecture/audit_schema_evolution.md
-- See docs/architecture/audit_dual_write_semantics.md

-- =============================================================================
-- 1. ADD STRUCTURED COLUMNS (all nullable for backward compatibility)
-- =============================================================================

ALTER TABLE audit_log ADD COLUMN event_type TEXT;
ALTER TABLE audit_log ADD COLUMN actor_id TEXT;
ALTER TABLE audit_log ADD COLUMN target_type TEXT;
ALTER TABLE audit_log ADD COLUMN target_id TEXT;
ALTER TABLE audit_log ADD COLUMN fiscal_year INTEGER;
ALTER TABLE audit_log ADD COLUMN before_snapshot TEXT;
ALTER TABLE audit_log ADD COLUMN after_snapshot TEXT;
ALTER TABLE audit_log ADD COLUMN node_id TEXT;
ALTER TABLE audit_log ADD COLUMN details TEXT;

-- =============================================================================
-- 2. NEW INDEXES FOR GOVERNANCE QUERIES
-- =============================================================================

CREATE INDEX IF NOT EXISTS idx_audit_target_entity
    ON audit_log(target_type, target_id, timestamp);

CREATE INDEX IF NOT EXISTS idx_audit_actor_timestamp
    ON audit_log(actor_id, timestamp);

CREATE INDEX IF NOT EXISTS idx_audit_fiscal_event
    ON audit_log(fiscal_year, event_type);

-- =============================================================================
-- 3. KEYSET PAGINATION INDEX
-- =============================================================================

CREATE INDEX IF NOT EXISTS idx_audit_keyset
    ON audit_log(timestamp, id);
