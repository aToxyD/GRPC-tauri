pub mod packages;
pub mod source_node;

pub use packages::{
    integrity::{PackageHasher, Sha256PackageHasher},
    read_admin_access_package_from_file,
    read_contract_catalog_package_from_file,
    // ── Preferred file-path readers (ADR-0016) ──────────────────────────
    read_daily_report_package_from_file,
    read_identity_access_package_from_file,
    read_monthly_summary_package_from_file,
    read_products_package_from_file,
    read_registry_package_from_file,
    read_stock_movements_package_from_file,
    read_trust_package_from_file,
    read_unit_node_package_from_file,
    signing::{PackageSigner, PackageVerifier},
    PackageBuilder,
    SerdeJsonSyncPackageDeserializer,
    SerdeJsonSyncPackageSerializer,
    SyncPackageSerializer,
};
pub use source_node::{infer_source_node_id, resolve_export_source_node_id};
