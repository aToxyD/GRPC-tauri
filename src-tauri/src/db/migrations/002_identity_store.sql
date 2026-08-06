-- =============================================================================
-- GRPC SQLite Schema Migration 002 — Identity Store
-- RFC 2026-08-04-node-identity-trust / ADR-0038
-- Version: 2
--
-- Additive-only. Does not alter any pre-existing table.
-- The Identity Store is the single source of truth for identity state.
-- Signing/verification material (public keys) is stored as an opaque BLOB;
-- all cryptographic operations live in infrastructure/security/identity.
-- =============================================================================

CREATE TABLE IF NOT EXISTS identity_store (
    identity_id TEXT PRIMARY KEY,
    subject_type TEXT NOT NULL CHECK(subject_type IN ('WILAYA', 'UNIT', 'ADMIN')),
    subject_id TEXT NOT NULL,
    issuer_identity_id TEXT,
    credential_id TEXT NOT NULL,
    generation INTEGER NOT NULL CHECK(generation >= 1),
    status TEXT NOT NULL CHECK(status IN ('ACTIVE', 'REVOKED', 'SUPERSEDED', 'EXPIRED')),
    public_key BLOB NOT NULL,
    algorithm_version INTEGER NOT NULL,
    not_after TEXT,
    package_sequence INTEGER,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    deleted INTEGER NOT NULL DEFAULT 0 CHECK(deleted IN (0, 1))
);

-- Invariant 6: exactly one ACTIVE credential per subject (WILAYA singleton included).
CREATE UNIQUE INDEX IF NOT EXISTS idx_identity_active_subject
    ON identity_store(subject_type, subject_id) WHERE status = 'ACTIVE' AND deleted = 0;

-- Invariants 2/3: credential lifecycle traversal (credential_id, generation).
CREATE INDEX IF NOT EXISTS idx_identity_credential_generation
    ON identity_store(credential_id, generation);

-- Trust chain traversal (issuer -> issued); NULL issuer = offline Authority Root.
CREATE INDEX IF NOT EXISTS idx_identity_issuer
    ON identity_store(issuer_identity_id);

-- Local node resolution (singleton subject types, e.g. WILAYA).
CREATE INDEX IF NOT EXISTS idx_identity_subject_type_status
    ON identity_store(subject_type, status, generation);
