//! Encoded interchange pipelines (serialization + encryption). No file I/O or transports here.
//!
//! ## Entry points
//!
//! For **reading** encrypted packages:
//! - Use `read_*_from_file` (preferred — O(plaintext) peak RAM, ADR-0016)
//!
//! For **writing** encrypted packages:
//! - Use `PackageBuilder::build_encrypted_stream_path` (streams directly to file)

pub mod canonical_json;
mod encrypted_package_reader;
pub mod integrity;
mod package_builder;
pub mod package_deserializer;
mod package_serializer;
pub mod signing;

pub use package_builder::PackageBuilder;
pub use package_deserializer::SerdeJsonSyncPackageDeserializer;
pub use package_serializer::{SerdeJsonSyncPackageSerializer, SyncPackageSerializer};

// ── Preferred file-path-based readers (ADR-0016) ──────────────────────────
pub use encrypted_package_reader::read_daily_report_package_from_file;
pub use encrypted_package_reader::read_admin_access_package_from_file;
pub use encrypted_package_reader::read_identity_access_package_from_file;
pub use encrypted_package_reader::read_monthly_summary_package_from_file;
pub use encrypted_package_reader::read_products_package_from_file;
pub use encrypted_package_reader::read_registry_package_from_file;
pub use encrypted_package_reader::read_stock_movements_package_from_file;
pub use encrypted_package_reader::read_trust_package_from_file;
pub use encrypted_package_reader::read_unit_node_package_from_file;

// Legacy byte-slice readers were removed in favor of the streaming file-based API.
