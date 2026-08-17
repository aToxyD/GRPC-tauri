//! Application Layer
//!
//! تحتوي على:
//! - Use cases (orchestration)
//! - Authorization policies (policy-based)
//! - Validation pipeline (DTO + domain invariants + business rules)

pub mod authz;
pub mod command_context;
pub mod oversight;
pub mod ports;
pub mod reporting;
pub mod services;
pub mod sync;
pub mod sync_integrity;
pub mod usecases;
