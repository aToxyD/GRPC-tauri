//! Identity & Access Synchronization payload (B8).
//!
//! Carried inside the signed V2 sync package (kind = `identity_access`),
//! one package per unit node. The payload carries the synchronized accounts
//! of exactly one unit: the fleet-wide `admin` and the unit-bound `user`.

use serde::{Deserialize, Serialize};

/// Payload of an Identity & Access package.
///
/// - `admin_password_hash` is derived in the fleet-wide admin domain
///   (GLOBAL_ADMIN_DOMAIN lives inside `Argon2PasswordHashProvider`), so the
///   same hash authenticates `admin` on every node.
/// - `user_password_hash` is node-bound to `unit_code`, so it authenticates
///   only on that unit.
/// - `*_enabled` encodes account status (`false` = disabled / soft-deleted).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IdentityAccessPayload {
    /// Canonical unit code the package targets.
    pub unit_code: String,
    /// Fleet-wide `admin` hash (admin derivation domain).
    pub admin_password_hash: String,
    /// `true` when the fleet `admin` account is enabled.
    pub admin_enabled: bool,
    /// Unit-bound `user` hash (node-bound to `unit_code`).
    pub user_password_hash: String,
    /// `true` when the unit `user` account is enabled.
    pub user_enabled: bool,
}

/// Payload of an Admin-Only B8 account synchronization package
/// (`kind = "admin_access"`, ADR-0051 — Accepted 2026-08-22).
///
/// WILAYA → all UNIT nodes, fleet-wide. The ABSENCE of a target unit is the
/// broadcast semantic: the same signed artifact is independently importable
/// by every authorized UNIT, and no sentinel target value is permitted.
///
/// `deny_unknown_fields` is the structural enforcement of the ADR-0051 §4
/// exclusion list: a payload carrying `unit_code`, a UNIT username/password/
/// hash, or any other operator-account material FAILS deserialization — it
/// cannot reach the apply boundary, let alone mutate an operator row.
/// The synchronized username itself is structurally canonical (`admin`,
/// hard-coded by the repository upsert) and is never transported.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AdminAccessPayload {
    /// Fleet-wide `admin` hash (admin derivation domain).
    pub admin_password_hash: String,
    /// `true` when the fleet `admin` account is enabled.
    pub admin_enabled: bool,
}
