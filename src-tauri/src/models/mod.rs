//! Models Module - Re-export Hub
//!
//! This module serves as the central hub for all domain models.
//! Models are organized by domain in separate files for better maintainability.

// Re-export audit models
pub use audit::*;

// Re-export DTOs
pub use dto::*;

// Sync interchange snapshots (schema DTOs)
pub use sync_snapshots::*;

// Re-export inventory models
pub use inventory::*;

// Re-export order models
pub use order::*;

pub use fifo::*;

// Re-export fiscal year models
pub use fiscal_year::*;

// Re-export identity & access synchronization payload
pub use identity_access::*;

// Re-export product models
pub use product::*;

// Re-export report models
pub use report::*;

// Re-export settings models
pub use settings::*;

// Re-export unit models
pub use unit::*;

// Re-export user models
pub use user::*;

// Re-export licensing contract types & DTOs (ADR-0042)
pub use licensing::*;

// Sub-modules
pub mod audit;
pub mod dto;
pub mod fifo;
pub mod fiscal_year;
pub mod identity_access;
pub mod inventory;
pub mod licensing;
pub mod order;
pub mod product;
pub mod report;
pub mod settings;
pub mod sync_snapshots;
pub mod unit;
pub mod user;
