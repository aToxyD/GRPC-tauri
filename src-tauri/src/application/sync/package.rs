//! Application-level sync package envelope: metadata + typed payload.
//!
//! Read-oriented denormalized envelope for interchange between nodes.
//! Not a domain entity; transport (binary, encrypted blob) lives in infrastructure.

use serde::{Deserialize, Serialize};

use super::package_metadata::SyncPackageMetadata;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncPackage<T> {
    pub metadata: SyncPackageMetadata,
    pub payload: T,
}
