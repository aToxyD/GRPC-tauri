//! Application Layer
//!
//! تحتوي على:
//! - Use cases (orchestration)
//! - Authorization policies (policy-based)
//! - Validation pipeline (DTO + domain invariants + business rules)

pub mod authz;
pub mod command_context;
pub mod ports;
pub mod services;
pub mod sync;
pub mod usecases;
