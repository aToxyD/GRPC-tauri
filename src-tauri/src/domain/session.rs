//! Session Management Module
//!
//! يدير جلسات المستخدمين مع دعم انتهاء الصلاحية التلقائي
//!
//! # Features
//! - Session timeout (30 دقيقة افتراضياً)
//! - تتبع آخر نشاط
//! - تحذير قبل انتهاء الجلسة (5 دقائق)

use crate::models::UserRole;
use chrono::{DateTime, Utc};
use uuid::Uuid;

/// مدة الجلسة الافتراضية (30 دقيقة)
pub const SESSION_TIMEOUT_MINUTES: i64 = 30;

/// وقت التحذير قبل انتهاء الجلسة (5 دقائق)
pub const SESSION_WARNING_MINUTES: i64 = 5;

/// Snapshot of stable user data captured at login.
/// Eliminates a DB round-trip on every get_current_user call.
/// Fields match exactly what the frontend consumes.
#[derive(Debug, Clone)]
pub struct UserSnapshot {
    pub id: String,
    pub username: String,
    pub role: UserRole,
    pub created_at: DateTime<Utc>,
}

/// معلومات الجلسة الحالية
#[derive(Debug, Clone)]
pub struct CurrentSession {
    pub user_id: String,
    pub username: String,
    pub user_role: UserRole,
    pub session_id: String,
    pub created_at: DateTime<Utc>,
    pub last_activity: DateTime<Utc>,
    pub timeout_minutes: i64,
    pub user_snapshot: UserSnapshot,
}

impl CurrentSession {
    /// إنشاء جلسة جديدة
    pub fn new(
        user_id: String,
        username: String,
        user_role: UserRole,
        user_snapshot: UserSnapshot,
    ) -> Self {
        let now = Utc::now();
        Self {
            user_id,
            username,
            user_role,
            session_id: Uuid::new_v4().to_string(),
            created_at: now,
            last_activity: now,
            timeout_minutes: SESSION_TIMEOUT_MINUTES,
            user_snapshot,
        }
    }

    /// تحديث وقت آخر نشاط
    pub fn touch(&mut self) {
        self.last_activity = Utc::now();
    }

    /// هل انتهت صلاحية الجلسة؟
    pub fn is_expired(&self) -> bool {
        let elapsed = Utc::now() - self.last_activity;
        elapsed.num_minutes() >= self.timeout_minutes
    }

    /// الدقائق المتبقية قبل انتهاء الجلسة
    pub fn remaining_minutes(&self) -> i64 {
        let elapsed = Utc::now() - self.last_activity;
        let remaining = self.timeout_minutes - elapsed.num_minutes();
        remaining.max(0)
    }

    /// هل يجب إظهار تحذير انتهاء الجلسة؟
    pub fn should_warn(&self) -> bool {
        self.remaining_minutes() <= SESSION_WARNING_MINUTES && !self.is_expired()
    }

    /// مدة الجلسة الكلية بالدقائق
    pub fn duration_minutes(&self) -> i64 {
        (Utc::now() - self.created_at).num_minutes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;
    use std::time::Duration;

    fn make_snapshot() -> UserSnapshot {
        UserSnapshot {
            id: "user_123".to_string(),
            username: "admin".to_string(),
            role: UserRole::Admin,
            created_at: Utc::now(),
        }
    }

    #[test]
    fn test_session_creation() {
        let snapshot = make_snapshot();
        let session = CurrentSession::new(
            "user_123".to_string(),
            "admin".to_string(),
            UserRole::Admin,
            snapshot,
        );

        assert_eq!(session.user_id, "user_123");
        assert_eq!(session.username, "admin");
        assert_eq!(session.user_role, UserRole::Admin);
        assert!(!session.session_id.is_empty());
        assert!(!session.is_expired());
        assert_eq!(session.timeout_minutes, SESSION_TIMEOUT_MINUTES);
    }

    #[test]
    fn test_session_touch_resets_timer() {
        let snapshot = make_snapshot();
        let mut session = CurrentSession::new(
            "user_123".to_string(),
            "admin".to_string(),
            UserRole::Admin,
            snapshot,
        );

        let first_activity = session.last_activity;
        sleep(Duration::from_millis(100));
        session.touch();

        assert!(session.last_activity > first_activity);
        assert!(!session.is_expired());
    }

    #[test]
    fn test_session_remaining_minutes() {
        let snapshot = make_snapshot();
        let session = CurrentSession::new(
            "user_123".to_string(),
            "admin".to_string(),
            UserRole::Admin,
            snapshot,
        );

        let remaining = session.remaining_minutes();
        assert!(remaining > 0);
        assert!(remaining <= SESSION_TIMEOUT_MINUTES);
    }

    #[test]
    fn test_session_duration_tracking() {
        let snapshot = make_snapshot();
        let session = CurrentSession::new(
            "user_123".to_string(),
            "admin".to_string(),
            UserRole::Admin,
            snapshot,
        );

        let duration = session.duration_minutes();
        assert!(duration >= 0);
    }

    #[test]
    fn test_session_should_warn() {
        let snapshot = make_snapshot();
        let mut session = CurrentSession::new(
            "user_123".to_string(),
            "admin".to_string(),
            UserRole::Admin,
            snapshot,
        );

        // New session should not warn
        assert!(!session.should_warn());

        // Simulate old session by manipulating last_activity
        session.last_activity = Utc::now() - chrono::Duration::minutes(26);
        assert!(session.should_warn());

        // Expired session should not warn (it's already expired)
        session.last_activity = Utc::now() - chrono::Duration::minutes(31);
        assert!(!session.should_warn());
    }
}
