//! Commands Module
//!
//! Modular organization of Tauri commands grouped by functionality.
//! This module re-exports all commands and types for the application.

pub mod audit_cmds;
pub mod auth;
pub mod backup;
pub mod common;
pub mod fiscal;
pub mod guards;
pub mod identity;
pub mod import_export;
pub mod integrity;
pub mod inventory;
pub mod observability;
pub mod operational;
pub mod orders;
pub mod procurement;
pub mod products;
pub mod registry;
pub mod reports;
pub mod security;
pub mod settings;
pub mod system;
pub mod types;
pub mod units;

// Re-export commonly used types
pub use types::AppState;

// Re-export guards
pub use guards::{
    authorize_command, enforce_forced_credential_state, FORCED_STATE_ALLOWED_COMMANDS,
};

// Re-export identity bootstrap/challenge commands (B5)
pub use identity::*;

// Re-export auth commands (including __cmd__ generated wrappers)
pub use auth::*;

// Re-export settings commands (including __cmd__ generated wrappers)
pub use settings::*;

// Re-export product commands (including __cmd__ generated wrappers)
pub use products::*;

// Re-export order commands (including __cmd__ generated wrappers)
pub use orders::*;

// Re-export unit commands (including __cmd__ generated wrappers)
pub use units::*;

// Re-export report commands (including __cmd__ generated wrappers)
pub use reports::*;

// Re-export inventory commands (including __cmd__ generated wrappers)
pub use inventory::*;

// Re-export backup commands (including __cmd__ generated wrappers)
pub use backup::*;

// Re-export import/export commands (including __cmd__ generated wrappers)
pub use import_export::*;

// Re-export procurement commands (ADR-0055 / SEC-087-F)
pub use procurement::*;

// Re-export integrity commands
pub use integrity::*;

// Re-export audit commands (including __cmd__ generated wrappers)
pub use audit_cmds::*;

// Re-export system commands (including __cmd__ generated wrappers)
pub use system::*;

// Re-export fiscal commands
pub use fiscal::*;

// Re-export observability commands
pub use observability::*;

// Re-export operational intelligence commands
pub use operational::*;

// Re-export application-key provisioning commands (ADR-0041)
pub use security::*;
