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
