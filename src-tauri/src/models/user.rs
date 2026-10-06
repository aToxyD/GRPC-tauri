//! User and Authentication Models
//!
//! User accounts, roles, sessions, and authentication-related types

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// User account with role-based access control
///
/// `password_hash` and `node_id` are **never** serialized to the frontend
/// (Tauri command responses), but they are retained in memory for auth.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: String,
    pub username: String,
    /// Never sent to the frontend — kept for in-process auth only.
    #[serde(skip_serializing)]
    pub password_hash: String,
    pub role: UserRole,
    pub created_at: DateTime<Utc>,
    /// Never sent to the frontend — kept for node-binding verification only.
    #[serde(skip_serializing)]
    pub node_id: String,
    /// B8 account status: `false` = enabled (deleted = 0), `true` = disabled
    /// (soft-deleted). Never sent to the frontend through the `User` DTO.
    #[serde(default, skip_serializing)]
    pub deleted: bool,
    /// ADR-0063 §5: persisted forced credential state. `true` means the stored
    /// credential must be replaced before normal application use; it is set
    /// when the canonical UNIT operator is minted with the bootstrap
    /// credential and cleared by the §6 self-change. Serialized to the
    /// frontend (owner: `sync.contract.ts` / SettingsPage) so the UI observes
    /// the backend's single source of truth — it is a projection of a
    /// boolean, not a credential secret.
    #[serde(default)]
    pub must_change_password: bool,
}

/// User role for authorization
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum UserRole {
    Admin,
    User,
}

impl std::fmt::Display for UserRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UserRole::Admin => write!(f, "Admin"),
            UserRole::User => write!(f, "User"),
        }
    }
}

impl From<String> for UserRole {
    fn from(s: String) -> Self {
        match s.as_str() {
            "Admin" => UserRole::Admin,
            _ => UserRole::User,
        }
    }
}

/// Login request payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

/// Login response with user data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginResponse {
    pub success: bool,
    pub user: Option<User>,
    pub message: String,
    pub requires_configuration: bool,
    /// B6-A (RFC 2026-08-04 / ADR-0038): true when this node has an ACTIVE
    /// ADMIN identity, making Challenge–Response (`.adminkey`) the mandatory
    /// login path. Additive with `#[serde(default)]` so pre-B6-A consumers of
    /// a cached response keep deserializing.
    #[serde(default)]
    pub identity_challenge_required: bool,
}

/// Session status for frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionStatus {
    pub is_active: bool,
    pub is_expired: bool,
    pub should_warn: bool,
    pub remaining_minutes: i64,
    pub username: Option<String>,
}

/// User data carried inside a sync unit-node package (`.unit`).
///
/// **Security contract (ADR-0012 adjacent):**
/// - `password_hash` MUST be included in both serialization and
///   deserialization because the hash must survive the Wilaya→Unit round-trip.
///   The node-binding re-hash happens inside `NodePackageService::import_unit_node_package`
///   *after* this struct is deserialized, using the receiving node's ID.
/// - This struct is NEVER returned to the frontend; it lives only
///   inside encrypted packages that are protected in transit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserExport {
    pub username: String,
    /// Included in the encrypted package for node re-binding on import.
    /// NOT exposed to the frontend.
    pub password_hash: String,
    pub role: String,
}

/// Login metrics for security monitoring
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LoginMetrics {
    pub total_attempts: u32,
    pub failed_attempts: u32,
    pub current_lockout: bool,
    pub lockout_reason: Option<String>,
    pub lockout_time_remaining: Option<u64>,
}

impl LoginMetrics {
    /// Format lockout time in Arabic
    pub fn format_lockout_time(&self) -> String {
        match self.lockout_time_remaining {
            Some(seconds) if seconds > 0 => {
                let minutes = seconds / 60;
                let secs = seconds % 60;
                if minutes > 0 {
                    format!("{} دقيقة و {} ثانية", minutes, secs)
                } else {
                    format!("{} ثانية", secs)
                }
            }
            _ => "غير محدد".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_role_display() {
        assert_eq!(UserRole::Admin.to_string(), "Admin");
        assert_eq!(UserRole::User.to_string(), "User");
    }

    #[test]
    fn test_user_role_from_string() {
        assert_eq!(UserRole::from("Admin".to_string()), UserRole::Admin);
        assert_eq!(UserRole::from("User".to_string()), UserRole::User);
        assert_eq!(UserRole::from("Unknown".to_string()), UserRole::User);
    }

    #[test]
    fn test_login_metrics_format() {
        let metrics = LoginMetrics {
            lockout_time_remaining: Some(125),
            ..Default::default()
        };
        assert!(metrics.format_lockout_time().contains("دقيقة"));
    }
}
