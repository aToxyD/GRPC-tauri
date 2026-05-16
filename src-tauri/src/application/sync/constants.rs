//! Wire format evolution for interchange packages (`SyncPackage`).
//!
//! Bumped when JSON shape or semantics break compatibility for consumers/importers.

use super::SchemaVersion;

pub const SYNC_PACKAGE_SCHEMA_VERSION: SchemaVersion = SchemaVersion::V2;
