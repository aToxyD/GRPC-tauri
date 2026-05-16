//! App Layer
//!
//! الهدف: إبقاء تكامل Tauri هنا فقط (adapters / state wiring).
//! - لا business logic
//! - لا authorization policies
//! - لا validation rules
//!
//! `state` يحتوي `AppState` و`CacheMetrics` — نقطة التجميع الوحيدة لحالة التطبيق.

pub mod state;
