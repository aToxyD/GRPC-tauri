//! Centralized Authorization (Policy-based)
//!
//! الهدف: توحيد الصلاحيات في طبقة واحدة قابلة للاختبار.
//! - لا DB access هنا
//! - لا Tauri types هنا

pub mod actions;
pub mod errors;
pub mod policies;
pub mod principal;
pub mod resource_context;

pub use actions::Action;
pub use errors::AuthorizationError;
pub use policies::authorize;
pub use principal::Principal;
pub use resource_context::ResourceContext;
