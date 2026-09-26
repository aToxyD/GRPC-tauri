//! Sync / interchange semantics at the application boundary.
//!
//! Defines what a “package” means between nodes (metadata, kind, payload envelope).
//! Serialization, encryption, and file I/O stay in `infrastructure`.

pub mod compatibility;
pub mod constants;
pub mod import;
pub mod import_provenance;
pub mod import_validation;
pub mod unit_issuer_membership;

mod package;
mod package_kind;
mod package_metadata;
mod schema_version;

pub use compatibility::{CompatibilityPolicy, ImportCompatibilityError, SupportedSchemaWindow};
pub use constants::SYNC_PACKAGE_SCHEMA_VERSION;
pub use import::{
    ImportAuditEvent, ImportAuditEventType, ImportAuditLogger, ImportFailureReason,
    ImportTransactionRunner, ImportedPackageRegistry,
};
pub use import_provenance::{
    monthly_summary_source_allowed_for_unit, products_source_allowed_for_unit,
    source_id_allowed_for_unit,
};
pub use import_validation::{
    validate_contract_catalog_package_for_import, validate_contract_fulfillment_package_for_import,
    validate_daily_report_package_for_import, validate_monthly_summary_package_for_import,
    validate_products_package_for_import, validate_products_package_unit_config_immutable,
};
pub use package::SyncPackage;
pub use package_kind::SyncPackageKind;
pub use package_metadata::{PackageExportMode, PackageId, SyncPackageMetadata};
pub use schema_version::SchemaVersion;
pub use unit_issuer_membership::verify_unit_issuer_membership;
