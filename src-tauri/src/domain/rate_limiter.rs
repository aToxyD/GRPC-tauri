//! Rate Limiter Module
//!
//! يوفر حماية ضد هجمات Brute Force عن طريق تقييد عدد المحاولات
//!
//! # Features
//! - حد أقصى للمحاولات (5 محاولات افتراضياً)
//! - فترة صلاحية (5 دقائق افتراضياً)
//! - تنظيف تلقائي للمدخلات القديمة

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// فترة الحظر بعد تجاوز المحاولات (5 دقائق)
const DEFAULT_WINDOW_SECS: u64 = 300;

/// الحد الأقصى للمحاولات المسموح بها
const DEFAULT_MAX_ATTEMPTS: u32 = 5;

/// معلومات تتبع المحاولات لكل مفتاح (IP أو username)
#[derive(Debug, Clone)]
struct AttemptInfo {
    /// عدد المحاولات الفاشلة في النافذة الحالية
    count: u32,
    /// عدد المحاولات الناجحة (يحفظ حتى بعد نجاح الدخول)
    successful_count: u32,
    /// إجمالي المحاولات الفاشلة تاريخياً (لا يُعاد تعيينه عند النجاح)
    total_failed_count: u32,
    /// وقت أول محاولة فاشلة في النافذة الحالية
    first_attempt: Instant,
    /// وقت آخر محاولة
    last_attempt: Instant,
}

impl AttemptInfo {
    fn new() -> Self {
        let now = Instant::now();
        Self {
            count: 1,
            successful_count: 0,
            total_failed_count: 1,
            first_attempt: now,
            last_attempt: now,
        }
    }

    fn new_with_success() -> Self {
        let now = Instant::now();
        Self {
            count: 0,
            successful_count: 1,
            total_failed_count: 0,
            first_attempt: now,
            last_attempt: now,
        }
    }

    /// التحقق مما إذا كانت النافذة قد انتهت
    fn is_window_expired(&self, window_duration: Duration) -> bool {
        self.first_attempt.elapsed() > window_duration
    }

    /// إعادة تعيين العداد (يحتفظ بالنجاحات)
    fn reset(&mut self) {
        let now = Instant::now();
        self.count = 1;
        self.first_attempt = now;
        self.last_attempt = now;
    }

    /// تسجيل محاولة ناجحة
    fn record_success(&mut self) {
        self.successful_count += 1;
        self.last_attempt = Instant::now();
    }

    /// زيادة عدد المحاولات
    fn increment(&mut self) {
        self.count += 1;
        self.total_failed_count += 1;
        self.last_attempt = Instant::now();
    }
}

/// مدير Rate Limiting
pub struct RateLimiter {
    /// مخزن المحاولات (مفتاح -> معلومات)
    attempts: Mutex<HashMap<String, AttemptInfo>>,
    /// فترة النافذة الزمنية
    window_duration: Duration,
    /// الحد الأقصى للمحاولات
    max_attempts: u32,
}

impl RateLimiter {
    /// إنشاء Rate Limiter جديد بالإعدادات الافتراضية
    pub fn new() -> Self {
        Self {
            attempts: Mutex::new(HashMap::new()),
            window_duration: Duration::from_secs(DEFAULT_WINDOW_SECS),
            max_attempts: DEFAULT_MAX_ATTEMPTS,
        }
    }

    /// إنشاء Rate Limiter بإعدادات مخصصة
    pub fn with_settings(window_secs: u64, max_attempts: u32) -> Self {
        Self {
            attempts: Mutex::new(HashMap::new()),
            window_duration: Duration::from_secs(window_secs),
            max_attempts,
        }
    }

    /// التحقق مما إذا كان المفتاح مسموحاً له بالمحاولة
    ///
    /// # Arguments
    /// * `key` - المفتاح (عادةً IP address أو username)
    ///
    /// # Returns
    /// * `true` إذا كان مسموحاً بالمحاولة
    /// * `false` إذا تم تجاوز الحد
    pub fn is_allowed(&self, key: &str) -> bool {
        let mut attempts = match self.attempts.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };

        // تنظيف المدخلات القديمة (كل 100 محاولة)
        if attempts.len() > 100 {
            self.cleanup_old_entries(&mut attempts);
        }

        if let Some(info) = attempts.get(key) {
            // إذا انتهت النافذة الزمنية، أعد التعيين
            if info.is_window_expired(self.window_duration) {
                return true;
            }

            // التحقق من عدم تجاوز الحد
            if info.count >= self.max_attempts {
                return false;
            }
        }

        true
    }

    /// تسجيل محاولة فاشلة
    ///
    /// # Arguments
    /// * `key` - المفتاح (IP address أو username)
    pub fn record_failure(&self, key: &str) {
        let mut attempts = match self.attempts.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };

        match attempts.get_mut(key) {
            Some(info) => {
                if info.is_window_expired(self.window_duration) {
                    info.reset();
                } else {
                    info.increment();
                }
            }
            None => {
                attempts.insert(key.to_string(), AttemptInfo::new());
            }
        }
    }

    /// تسجيل محاولة ناجحة (يحتفظ بالسجل)
    ///
    /// # Arguments
    /// * `key` - المفتاح (IP address أو username)
    pub fn record_success(&self, key: &str) {
        let mut attempts = match self.attempts.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };

        match attempts.get_mut(key) {
            Some(info) => {
                info.record_success();
                // Reset failed count after successful login
                info.count = 0;
            }
            None => {
                attempts.insert(key.to_string(), AttemptInfo::new_with_success());
            }
        }
    }

    /// الحصول على إجمالي المحاولات الناجحة لجميع المستخدمين
    pub fn get_total_successful_attempts(&self) -> u32 {
        let attempts = match self.attempts.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        attempts.values().map(|info| info.successful_count).sum()
    }

    /// الحصول على إجمالي جميع المحاولات (ناجحة + فاشلة)
    pub fn get_total_attempts(&self) -> u32 {
        let attempts = match self.attempts.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        attempts
            .values()
            .map(|info| info.total_failed_count + info.successful_count)
            .sum()
    }

    /// الحصول على الوقت المتبقي للحظر (بالثواني)
    ///
    /// # Arguments
    /// * `key` - المفتاح (IP address أو username)
    ///
    /// # Returns
    /// * `Some(u64)` - الوقت المتبقي بالثواني
    /// * `None` - إذا لم يكن المفتاح محظوراً
    pub fn get_remaining_lockout_secs(&self, key: &str) -> Option<u64> {
        let attempts = match self.attempts.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };

        if let Some(info) = attempts.get(key) {
            if info.count >= self.max_attempts && !info.is_window_expired(self.window_duration) {
                let elapsed = info.first_attempt.elapsed();
                let remaining = self.window_duration.saturating_sub(elapsed);
                return Some(remaining.as_secs());
            }
        }

        None
    }

    /// الحصول على عدد المحاولات المتبقية
    ///
    /// # Arguments
    /// * `key` - المفتاح (IP address أو username)
    ///
    /// # Returns
    /// * `u32` - عدد المحاولات المتبقية
    pub fn get_remaining_attempts(&self, key: &str) -> u32 {
        let attempts = match self.attempts.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };

        if let Some(info) = attempts.get(key) {
            if info.is_window_expired(self.window_duration) {
                return self.max_attempts;
            }
            return self.max_attempts.saturating_sub(info.count);
        }

        self.max_attempts
    }

    /// تنظيف المدخلات القديمة
    fn cleanup_old_entries(&self, attempts: &mut HashMap<String, AttemptInfo>) {
        let keys_to_remove: Vec<String> = attempts
            .iter()
            .filter(|(_, info)| info.is_window_expired(self.window_duration))
            .map(|(key, _)| key.clone())
            .collect();

        for key in keys_to_remove {
            attempts.remove(&key);
        }
    }

    /// إعادة تعيين جميع العدادات (مفيد للاختبارات)
    pub fn reset_all(&self) {
        let mut attempts = match self.attempts.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        attempts.clear();
    }

    /// الحصول على إجمالي المحاولات الفاشلة لجميع المستخدمين (تاريخياً)
    pub fn get_total_failed_attempts(&self) -> u32 {
        let attempts = match self.attempts.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        attempts.values().map(|info| info.total_failed_count).sum()
    }

    /// الحصول على عدد المستخدمين المسجلين حالياً (المحظورين أو غير المحظورين)
    pub fn get_tracked_users_count(&self) -> usize {
        let attempts = match self.attempts.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        attempts.len()
    }

    /// الحصول على عدد المستخدمين المحظورين حالياً
    pub fn get_locked_users_count(&self) -> usize {
        let attempts = match self.attempts.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        attempts
            .values()
            .filter(|info| {
                info.count >= self.max_attempts && !info.is_window_expired(self.window_duration)
            })
            .count()
    }
    /// الحصول على إحصائيات الدخول
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
        let limiter = RateLimiter::with_settings(60, 3); // 3 محاولات في دقيقة

        // 3 محاولات فاشلة
        limiter.record_failure("user1");
        limiter.record_failure("user1");
        limiter.record_failure("user1");

        // الرابعة يجب أن تُمنع
        assert!(!limiter.is_allowed("user1"));
    }

    #[test]
    fn test_rate_limiter_resets_after_success() {
        let limiter = RateLimiter::with_settings(60, 2);

        limiter.record_failure("user1");
        limiter.record_failure("user1");
        assert!(!limiter.is_allowed("user1"));

        // محاولة ناجحة تعيد التعيين
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

        // user1 يتجاوز الحد
        limiter.record_failure("user1");
        limiter.record_failure("user1");
        assert!(!limiter.is_allowed("user1"));

        // user2 غير متأثر
        assert!(limiter.is_allowed("user2"));
    }
}
