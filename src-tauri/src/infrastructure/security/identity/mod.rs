//! Identity crypto adapters.
//!
//! RFC 2026-08-04-node-identity-trust / ADR-0038.
//!
//! Concrete Ed25519 (RFC 8032) signing/verification providers implementing the
//! domain ports. Wired into Challenge–Response and package signing in B3/B4.

pub mod ed25519;

pub use ed25519::{Ed25519SignatureVerifier, Ed25519SigningProvider};
