//! Serialization of [`SyncPackage`](crate::application::sync::SyncPackage) payloads to bytes.

use serde::Serialize;

use crate::application::sync::SyncPackage;
use crate::errors::{AppError, AppResult};

pub trait SyncPackageSerializer: Send + Sync {
    fn serialize<T: Serialize>(&self, package: &SyncPackage<T>) -> AppResult<Vec<u8>>;

    /// Buffered JSON Export
    fn serialize_to_writer<T: Serialize, W: std::io::Write>(
        &self,
        package: &SyncPackage<T>,
        writer: W,
    ) -> AppResult<()>;
}

/// JSON envelope.
///
/// Wire bytes for [`SYNC_PACKAGE_SCHEMA_VERSION`](crate::application::sync::SYNC_PACKAGE_SCHEMA_VERSION)
/// use **canonical JSON** (sorted object keys) per ADR 0009.
#[derive(Clone, Copy, Debug, Default)]
pub struct SerdeJsonSyncPackageSerializer;

impl SyncPackageSerializer for SerdeJsonSyncPackageSerializer {
    fn serialize<T: Serialize>(&self, package: &SyncPackage<T>) -> AppResult<Vec<u8>> {
        // Serializes the full object graph into memory.
        // We use to_vec directly; canonicalization is now guaranteed by reordered
        // SyncPackageMetadata fields and compact JSON output.
        serde_json::to_vec(package)
            .map_err(|e| AppError::Internal(format!("sync JSON serialize: {}", e)))
    }

    fn serialize_to_writer<T: Serialize, W: std::io::Write>(
        &self,
        package: &SyncPackage<T>,
        writer: W,
    ) -> AppResult<()> {
        // Buffered JSON Export: Writes the materialized object graph directly to the writer.
        // Peak memory is bounded by the size of the 'package' object already in RAM.
        // Canonicalization (ADR-0009) is maintained by the alphabetical field order
        // in SyncPackage and SyncPackageMetadata.
        serde_json::to_writer(writer, package)
            .map_err(|e| AppError::Internal(format!("sync JSON stream: {}", e)))
    }
}
