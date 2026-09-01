//! Rate Limiter Module
//!
//! يوفر حماية ضد هجمات Brute Force عن طريق تقييد عدد المحاولات
//!
//! # Features
//! - حد أقصى للمحاولات (5 محاولات افتراضياً)
//! - فترة صلاحية (5 دقائق افتراضياً)
//! - تنظيف تلقائي للمدخلات القديمة
//! - تخزين مستمر في SQLite عبر RateLimiterStore (للمحافظة على الإعدادات عبر إعادة التشغيل)

use crate::domain::ports::rate_limiter_store::{
    InMemoryRateLimiterStore, PersistedAttemptInfo, RateLimiterStore,
};
use std::time::{SystemTime, UNIX_EPOCH};

/// فترة الحظر بعد تجاوز المحاولات (5 دقائق)
const DEFAULT_WINDOW_SECS: u64 = 300;

/// الحد الأقصى للمحاولات المسموح بها
const DEFAULT_MAX_ATTEMPTS: u32 = 5;

/// Reserved store key for the per-node global login breaker (SEC-003-07).
///
/// Counts failed password-login attempts across ALL usernames within one
/// window. Not a real user — the aggregate login metrics exclude it.
const GLOBAL_BREAKER_KEY: &str = "__grpc_global_login_breaker__";

/// SEC-003-07: threshold of FAILED logins across all usernames within one
/// window (`DEFAULT_WINDOW_SECS`) that trips a node-wide temporary login
/// backoff.
///
/// Rationale: per-user max is `DEFAULT_MAX_ATTEMPTS` (5). 20 distributed
/// failures means an attacker can no longer evade the per-user limiter by
/// spraying across many accounts (≥4 users at their per-user limit, or 5 users
/// at 4 each) without ALSO tripping the node-wide breaker. Conservative and
/// small to bound the recovery latency (the breaker recovers via window
/// expiry, not on any single successful login).
const DEFAULT_GLOBAL_MAX_ATTEMPTS: u32 = 20;

fn now_epoch() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

/// معلومات تتبع المحاولات لكل مفتاح (IP أو username)
#[derive(Debug, Clone)]
struct AttemptInfo {
    count: u32,
    successful_count: u32,
    total_failed_count: u32,
    /// Unix epoch seconds of first failed attempt in the current window
    first_attempt_at: i64,
    /// Unix epoch seconds of most recent attempt
    last_attempt_at: i64,
}

impl AttemptInfo {
    fn new() -> Self {
        let now = now_epoch();
        Self {
            count: 1,
            successful_count: 0,
            total_failed_count: 1,
            first_attempt_at: now,
            last_attempt_at: now,
        }
    }

    fn new_with_success() -> Self {
        let now = now_epoch();
        Self {
            count: 0,
            successful_count: 1,
            total_failed_count: 0,
            first_attempt_at: now,
            last_attempt_at: now,
        }
    }

    fn is_window_expired(&self, window_secs: u64) -> bool {
        let elapsed = now_epoch().saturating_sub(self.first_attempt_at) as u64;
        elapsed > window_secs
    }

    fn reset(&mut self) {
        let now = now_epoch();
        self.count = 1;
        self.first_attempt_at = now;
        self.last_attempt_at = now;
    }

    fn record_success(&mut self) {
        self.successful_count += 1;
        self.last_attempt_at = now_epoch();
    }

    fn increment(&mut self) {
        self.count += 1;
        self.total_failed_count += 1;
        self.last_attempt_at = now_epoch();
    }
}

impl From<&AttemptInfo> for PersistedAttemptInfo {
    fn from(info: &AttemptInfo) -> Self {
        Self {
            count: info.count,
            successful_count: info.successful_count,
            total_failed_count: info.total_failed_count,
            first_attempt_at: info.first_attempt_at,
            last_attempt_at: info.last_attempt_at,
        }
    }
}

impl From<PersistedAttemptInfo> for AttemptInfo {
    fn from(info: PersistedAttemptInfo) -> Self {
        Self {
            count: info.count,
            successful_count: info.successful_count,
            total_failed_count: info.total_failed_count,
            first_attempt_at: info.first_attempt_at,
            last_attempt_at: info.last_attempt_at,
        }
    }
}

/// مدير Rate Limiting
pub struct RateLimiter {
    store: Box<dyn RateLimiterStore>,
    window_secs: u64,
    max_attempts: u32,
    global_max_attempts: u32,
}

impl RateLimiter {
    /// إنشاء Rate Limiter جديد بالإعدادات الافتراضية (في الذاكرة)
    pub fn new() -> Self {
        Self {
            store: Box::new(InMemoryRateLimiterStore::new()),
            window_secs: DEFAULT_WINDOW_SECS,
            max_attempts: DEFAULT_MAX_ATTEMPTS,
            global_max_attempts: DEFAULT_GLOBAL_MAX_ATTEMPTS,
        }
    }

    /// إنشاء Rate Limiter بإعدادات مخصصة (في الذاكرة)
    pub fn with_settings(window_secs: u64, max_attempts: u32) -> Self {
        Self {
            store: Box::new(InMemoryRateLimiterStore::new()),
            window_secs,
            max_attempts,
            global_max_attempts: DEFAULT_GLOBAL_MAX_ATTEMPTS,
        }
    }

    /// إنشاء Rate Limiter مع مخزن محدد (للإنتاجية مع SQLite)
    pub fn with_store(
        store: Box<dyn RateLimiterStore>,
        window_secs: u64,
        max_attempts: u32,
    ) -> Self {
        Self {
            store,
            window_secs,
            max_attempts,
            global_max_attempts: DEFAULT_GLOBAL_MAX_ATTEMPTS,
        }
    }

    pub fn is_allowed(&self, key: &str) -> bool {
        self.is_allowed_for(key, self.max_attempts)
    }

    fn is_allowed_for(&self, key: &str, max_attempts: u32) -> bool {
        if let Ok(Some(tracked)) = self.store.get_attempt(key) {
            let info = AttemptInfo::from(tracked);
            if info.is_window_expired(self.window_secs) {
                return true;
            }
            if info.count >= max_attempts {
                return false;
            }
        }
        true
    }

    pub fn record_failure(&self, key: &str) {
        let mut info = match self.store.get_attempt(key) {
            Ok(Some(existing)) => AttemptInfo::from(existing),
            _ => {
                let _ = self
                    .store
                    .upsert_attempt(key, &PersistedAttemptInfo::from(&AttemptInfo::new()));
                return;
            }
        };

        if info.is_window_expired(self.window_secs) {
            info.reset();
        } else {
            info.increment();
        }
        let _ = self
            .store
            .upsert_attempt(key, &PersistedAttemptInfo::from(&info));
    }

    pub fn record_success(&self, key: &str) {
        let mut info = match self.store.get_attempt(key) {
            Ok(Some(existing)) => AttemptInfo::from(existing),
            _ => {
                let _ = self.store.upsert_attempt(
                    key,
                    &PersistedAttemptInfo::from(&AttemptInfo::new_with_success()),
                );
                return;
            }
        };

        info.record_success();
        info.count = 0;
        let _ = self
            .store
            .upsert_attempt(key, &PersistedAttemptInfo::from(&info));
    }

    // ── SEC-003-07: per-node global login breaker ───────────────────────────
    // A local attacker who spreads failed logins across many usernames evades
    // the per-user limiter. The node-wide breaker counts failures across ALL
    // usernames within the window and temporarily rejects logins node-wide.
    //
    // It is intentionally consulted ONLY by the password login command
    // (`commands::auth::login`). The WILAYA Challenge–Response path keeps its
    // own per-key bucket (`CHALLENGE_RATE_LIMIT_KEY`) and never touches the
    // global breaker — its `is_allowed`/`record_failure`/`record_success`
    // calls operate strictly per-key, so this breaker does not affect it.

    /// True while no node-wide login backoff is active (SEC-003-07).
    pub fn is_global_login_allowed(&self) -> bool {
        self.is_allowed_for(GLOBAL_BREAKER_KEY, self.global_max_attempts)
    }

    /// Record a FAILED password login. Counts toward the per-user bucket AND
    /// the node-wide breaker (SEC-003-07). Successful logins keep the existing
    /// per-user reset semantics; the global breaker recovers via window expiry.
    pub fn record_login_failure(&self, username: &str) {
        self.record_failure(username);
        self.record_failure(GLOBAL_BREAKER_KEY);
    }

    /// Remaining node-wide backoff seconds, if the global breaker is active.
    pub fn get_global_remaining_lockout_secs(&self) -> Option<u64> {
        self.get_remaining_lockout_secs_for(GLOBAL_BREAKER_KEY, self.global_max_attempts)
    }

    pub fn get_total_successful_attempts(&self) -> u32 {
        self.user_attempts()
            .iter()
            .map(|(_, info)| info.successful_count)
            .sum()
    }

    pub fn get_total_attempts(&self) -> u32 {
        self.user_attempts()
            .iter()
            .map(|(_, info)| info.total_failed_count + info.successful_count)
            .sum()
    }

    pub fn get_remaining_lockout_secs(&self, key: &str) -> Option<u64> {
        self.get_remaining_lockout_secs_for(key, self.max_attempts)
    }

    fn get_remaining_lockout_secs_for(&self, key: &str, max_attempts: u32) -> Option<u64> {
        match self.store.get_attempt(key) {
            Ok(Some(info)) => {
                let now = now_epoch();
                if info.count >= max_attempts {
                    let elapsed = now.saturating_sub(info.first_attempt_at) as u64;
                    if elapsed < self.window_secs {
                        return Some(self.window_secs.saturating_sub(elapsed));
                    }
                }
                None
            }
            _ => None,
        }
    }

    pub fn get_remaining_attempts(&self, key: &str) -> u32 {
        match self.store.get_attempt(key) {
            Ok(Some(info)) => {
                let now = now_epoch();
                if now.saturating_sub(info.first_attempt_at) as u64 > self.window_secs {
                    return self.max_attempts;
                }
                self.max_attempts.saturating_sub(info.count)
            }
            _ => self.max_attempts,
        }
    }

    pub fn reset_all(&self) {
        let _ = self.store.cleanup_old_entries(0);
    }

    /// Checkpoint the underlying store before shutdown to ensure WAL data is persisted.
    pub fn shutdown(&self) {
        if let Err(e) = self.store.checkpoint() {
            log::warn!(target: "grpc::runtime", "rate limiter checkpoint failed on shutdown: {}", e);
        }
    }

    /// All tracked per-user attempts, excluding the reserved global breaker key
    /// (SEC-003-07) so node-wide failures never pollute user-facing metrics.
    fn user_attempts(&self) -> Vec<(String, PersistedAttemptInfo)> {
        self.store
            .get_all_attempts()
            .unwrap_or_default()
            .into_iter()
            .filter(|(key, _)| key != GLOBAL_BREAKER_KEY)
            .collect()
    }

    pub fn get_total_failed_attempts(&self) -> u32 {
        self.user_attempts()
            .iter()
            .map(|(_, info)| info.total_failed_count)
            .sum()
    }

    pub fn get_tracked_users_count(&self) -> usize {
        self.user_attempts().len()
    }

    pub fn get_locked_users_count(&self) -> usize {
        let now = now_epoch();
        self.user_attempts()
            .iter()
            .filter(|(_, info)| {
                info.count >= self.max_attempts
                    && (now.saturating_sub(info.first_attempt_at) as u64) <= self.window_secs
            })
            .count()
    }

    pub fn get_metrics(&self) -> crate::models::LoginMetrics {
        crate::application::services::SystemStatsService::get_login_metrics(self)
    }
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_limiter_allows_initial_attempts() {
        let limiter = RateLimiter::new();
        assert!(limiter.is_allowed("user1"));
        assert!(limiter.is_allowed("user1"));
        assert!(limiter.is_allowed("user1"));
    }

    #[test]
    fn test_rate_limiter_blocks_after_max_attempts() {
        let limiter = RateLimiter::with_settings(60, 3);

        limiter.record_failure("user1");
        limiter.record_failure("user1");
        limiter.record_failure("user1");

        assert!(!limiter.is_allowed("user1"));
    }

    #[test]
    fn test_rate_limiter_resets_after_success() {
        let limiter = RateLimiter::with_settings(60, 2);

        limiter.record_failure("user1");
        limiter.record_failure("user1");
        assert!(!limiter.is_allowed("user1"));

        limiter.record_success("user1");
        assert!(limiter.is_allowed("user1"));
    }

    #[test]
    fn test_remaining_attempts() {
        let limiter = RateLimiter::with_settings(60, 5);

        assert_eq!(limiter.get_remaining_attempts("user1"), 5);

        limiter.record_failure("user1");
        assert_eq!(limiter.get_remaining_attempts("user1"), 4);

        limiter.record_failure("user1");
        assert_eq!(limiter.get_remaining_attempts("user1"), 3);
    }

    #[test]
    fn test_different_keys_independent() {
        let limiter = RateLimiter::with_settings(60, 2);

        limiter.record_failure("user1");
        limiter.record_failure("user1");
        assert!(!limiter.is_allowed("user1"));

        assert!(limiter.is_allowed("user2"));
    }

    // ── SEC-003-07: per-node global login breaker ──────────────────────────

    #[test]
    fn test_sec007_per_user_limiter_unchanged() {
        let limiter = RateLimiter::with_settings(60, 3);

        limiter.record_login_failure("user1");
        limiter.record_login_failure("user1");
        limiter.record_login_failure("user1");

        assert!(
            !limiter.is_allowed("user1"),
            "per-user limit must still trip"
        );
        assert!(limiter.is_allowed("user2"), "other users unaffected");
        assert!(
            limiter.is_global_login_allowed(),
            "3 failures below the 20-failure node-wide threshold"
        );
    }

    #[test]
    fn test_sec007_distributed_failures_trip_global_breaker() {
        let limiter = RateLimiter::new();

        for i in 0..20 {
            limiter.record_login_failure(&format!("attacker{i}"));
        }

        assert!(
            !limiter.is_global_login_allowed(),
            "distributed failures must trip the node-wide breaker"
        );
        assert!(
            limiter.get_global_remaining_lockout_secs().is_some(),
            "active breaker must report remaining backoff"
        );
    }

    #[test]
    fn test_sec007_global_breaker_rejects_all_logins_while_active() {
        let limiter = RateLimiter::new();

        for i in 0..20 {
            limiter.record_login_failure(&format!("attacker{i}"));
        }

        assert!(!limiter.is_global_login_allowed());
        // A fresh victim still has a clean per-user bucket, but `login()` checks
        // `is_global_login_allowed()` FIRST and rejects the attempt node-wide.
        assert!(limiter.is_allowed("victim"), "per-user bucket is clean");
        assert!(
            !limiter.is_global_login_allowed(),
            "node-wide login is rejected"
        );
    }

    #[test]
    fn test_sec007_global_breaker_recovers_after_window() {
        let limiter = RateLimiter::with_settings(1, 5);

        for i in 0..20 {
            limiter.record_login_failure(&format!("attacker{i}"));
        }
        assert!(!limiter.is_global_login_allowed());

        // `now_epoch()` truncates to whole seconds, so a 1s window needs a
        // ~2s sleep before the window is strictly exceeded.
        std::thread::sleep(std::time::Duration::from_millis(2500));
        assert!(
            limiter.is_global_login_allowed(),
            "breaker must recover after the window expires"
        );
    }

    #[test]
    fn test_sec007_successful_login_resets_per_user_not_global() {
        let limiter = RateLimiter::new();

        for i in 0..20 {
            limiter.record_login_failure(&format!("attacker{i}"));
        }
        assert!(!limiter.is_global_login_allowed());

        limiter.record_success("attacker0");
        assert!(
            limiter.is_allowed("attacker0"),
            "per-user success keeps its existing reset semantics"
        );
        assert!(
            !limiter.is_global_login_allowed(),
            "global breaker recovers only via window expiry"
        );
    }

    #[test]
    fn test_sec007_challenge_path_unaffected_by_global_breaker() {
        let limiter = RateLimiter::new();

        for _ in 0..8 {
            limiter.record_failure("admin");
        }

        assert!(
            !limiter.is_allowed("admin"),
            "per-key bucket locks normally"
        );
        assert!(
            limiter.is_global_login_allowed(),
            "Challenge–Response failures must NOT trip the node-wide breaker"
        );
    }

    #[test]
    fn test_sec007_metrics_exclude_global_breaker_key() {
        let limiter = RateLimiter::with_settings(60, 3);

        limiter.record_login_failure("user1");
        limiter.record_login_failure("user2");

        assert_eq!(
            limiter.get_total_failed_attempts(),
            2,
            "global key must not pollute failed-attempts metrics"
        );
        assert_eq!(
            limiter.get_total_attempts(),
            2,
            "global key must not pollute total-attempts metrics"
        );
        assert_eq!(
            limiter.get_tracked_users_count(),
            2,
            "global key is not a user"
        );
    }
}
