-- Registry fleet-state snapshots (RFC 2026-08-04 §3.9, B4).
-- Additive: stores each accepted Registry Package payload for auditability
-- (P4) and deterministic replay. Unit-management mutation stays out of scope.
CREATE TABLE IF NOT EXISTS registry_snapshots (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    package_id TEXT NOT NULL UNIQUE,
    snapshot_version INTEGER NOT NULL,
    wilaya_identity_id TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    imported_at TEXT NOT NULL,
    imported_by TEXT NOT NULL
);
