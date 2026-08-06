-- =============================================================================
-- GRPC SQLite Schema Migration 003 — Certificate signature column
-- RFC 2026-08-04-node-identity-trust / ADR-0038 / ADR-0039
-- Version: 3
--
-- Additive-only. Does not alter any pre-existing column or index.
--
-- `signature` is NULL ONLY during the legacy migration window (records issued
-- before Identity Trust activation may carry no signature). NULL is a migration
-- mechanism, NOT part of the final model: a later additive migration will
-- enforce NOT NULL once V1 support is removed (ADR-0039 §6).
--
-- The signature is an opaque Ed25519 BLOB (64 bytes); the domain enforces the
-- fixed length at the boundary via Ed25519CertificateSignature. All
-- cryptographic operations live in infrastructure/security/identity.
-- =============================================================================

ALTER TABLE identity_store ADD COLUMN signature BLOB;
