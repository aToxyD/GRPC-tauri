//! Node Identity infrastructure — secret stores and crypto adapters.
//!
//! ADR-0039 two-tier secret model:
//! - Node-managed secrets (WILAYA/UNIT signing keys) → `node_key_store` using
//!   `age::x25519` bound to `GRPC_APP_KEY`.
//! - Portable operator key (`.adminkey`) → `adminkey_provider` using
//!   `age::scrypt` — the ONLY legal scrypt site in the codebase (Rule 38).
//!
//! Challenge–Response signing/verification is delegated to the Ed25519
//! providers in `infrastructure::security::identity`.

pub mod adminkey_provider;
pub mod data_dir;
pub mod node_key_store;
pub mod root_public_key;

pub use adminkey_provider::{AdminKeyProvider, ADMINKEY_FILE_NAME, GRPC_DATA_DIR};
pub use data_dir::{identity_data_dir, IDENTITY_DATA_DIR_ENV, IDENTITY_FILE_NAMES};
pub use node_key_store::{NodeKeyStore, NODE_KEY_FILE_NAME};
pub use root_public_key::resolve_root_public_key;
