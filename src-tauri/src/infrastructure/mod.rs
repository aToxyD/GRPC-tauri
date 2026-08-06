//! Infrastructure Layer (gradual introduction)
//!
//! سيتم نقل adapters concrete هنا تدريجيًا.
//! حاليًا: placeholder لتثبيت حدود المعمارية بدون إعادة كتابة.

pub mod backup;
pub mod db;
pub mod export;
pub mod identity;
pub mod logging;
pub mod security;
pub mod sqlite_observability;
pub mod sqlite_runtime;
pub mod sqlite_runtime_review;
pub mod sync;

/// الحد الأقصى لحجم ملفات الاستيراد (512 ميجابايت) لمنع استنزاف الذاكرة.
pub const MAX_IMPORT_SIZE: u64 = 512 * 1024 * 1024;
