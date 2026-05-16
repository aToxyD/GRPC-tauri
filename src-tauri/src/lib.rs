#![deny(clippy::todo)]
#![warn(clippy::complexity)]

pub mod architecture;
pub mod db;
pub mod errors;
pub mod models;

/// App layer - Tauri adapters and state wiring
pub mod app;

/// Application layer - usecases, authorization, validation orchestration
pub mod application;

/// Domain layer - core business logic, handlers, and utilities
pub mod domain;

/// Command layer - Tauri command handlers with authorization and audit logging
pub mod commands;

/// Repository layer - database access abstractions
pub mod repositories;

/// Infrastructure layer - concrete implementations (DB, filesystem, crypto adapters)
///
/// Note: This is introduced gradually; existing code remains in `db/` + `repositories/` for now.
pub mod infrastructure;
