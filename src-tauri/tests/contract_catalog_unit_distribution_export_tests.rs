//! SEC-087 Phase 2 — C3: UNIT-scoped Contract Catalog export construction.
//!
//! Proves (ADR-0059):
//! - `export_contract_catalog_unit_distribution` emits one INDEPENDENT
//!   signed/encrypted artifact per requested target — never a clone of a
//!   fleet-wide artifact — with authenticated `(UnitDistribution, code)`
//!   package metadata;
//! - unit scope is enforced at the SQL boundary: contracts (Accepted | Active |
//!   Ended only), allocations (via the target-owned contract chain AND the
//!   row's own unit), exceptions, suppliers and unit–supplier links are all
//!   restricted to the target; tax policies stay global;
//! - B-only defects never invalidate A's artifact (selected-dataset
//!   construction precedes validation);
//! - the whole batch fails closed when any requested target is unresolvable or
//!   any selected dataset is invalid — no partial artifact set is emitted;
//! - `FleetRestore` semantics remain unchanged (all statuses, unscoped) and the
//!   mode-target pair (-) is always explicitly labeled in the signed envelope;
//! - repeated exports are deterministic in DATASET content (distinct
//!   package_id / signature only);
//! - the persisted reproducibility snapshot carries the SAME semantics as the
//!   package metadata (`"fleet_restore"`+null / `"unit_distribution"`+code).

use std::collections::HashSet;

use tempfile::TempDir;
use uuid::Uuid;

use grpc_lib::application::services::export_contract_catalog_unit_distribution;
use grpc_lib::application::services::{
    current_wilaya_signing_key_id, export_contract_catalog_fleet,
    record_export_with_reproducibility, ExportReproducibilityContext,
    FinalizeWilayaProvisionResult, IdentityProvisioningService,
    SyncPackageIdentityVerificationService,
};
use grpc_lib::application::sync::PackageExportMode;
use grpc_lib::db::{ConnectionFactory, Database};
use grpc_lib::domain::identity::{
    Ed25519CertificateSignature, IdentityCertificate, IdentitySigner, SubjectType,
};
use grpc_lib::infrastructure::identity::NodeKeyStore;
use grpc_lib::infrastructure::security::file_encryption::AgeFileEncryptionProvider;
use grpc_lib::infrastructure::security::Ed25519SigningProvider;
use grpc_lib::infrastructure::sync::read_contract_catalog_package_from_file;
use grpc_lib::models::{
    CreateContractRequest, CreateSupplierRequest, Product, ProductUnitConfigCodes,
    ReleaseReasonCode, SetTaxPolicyRequest,
};
use grpc_lib::repositories::executor::DbExecutor;
use grpc_lib::repositories::RepositoryProvider;

const FIXED_NOW: &str = "2026-08-04T00:00:00Z";
const WILAYA: &str = "16";

/// RFC 8032 §7.1 TEST 1 secret — matches the debug-mode Root fallback.
const TEST_ROOT_SECRET: [u8; 32] = [
    0x9d, 0x61, 0xb1, 0x9d, 0xef, 0xfd, 0x5a, 0x60, 0xba, 0x84, 0x4a, 0xf4, 0x92, 0xec, 0x2c, 0xc4,
    0x44, 0x49, 0xc5, 0x69, 0x7b, 0x32, 0x69, 0x19, 0x70, 0x3b, 0xac, 0x03, 0x1c, 0xae, 0x7f, 0x60,
];

struct Node {
    _dir: TempDir,
    db: Database,
    node_key_store: NodeKeyStore,
}

fn fresh_node() -> Node {
    let dir = TempDir::new().expect("temp dir");
    let db = ConnectionFactory::new_for_test().expect("db");
    let node_key_store = NodeKeyStore::new(dir.path().join("node"));
    Node {
        _dir: dir,
        db,
        node_key_store,
    }
}

fn make_executor(db: &Database) -> DbExecutor<'_> {
    db.executor()
}

fn root_signer() -> Ed25519SigningProvider {
    Ed25519SigningProvider::new(TEST_ROOT_SECRET)
}

/// Full offline WILAYA bootstrap (Root-issued). Returns the ACTIVE WILAYA cert.
fn bootstrap_wilaya(node: &mut Node) -> IdentityCertificate {
    let mut provisioning = IdentityProvisioningService::new(&mut node.db);
    let request = provisioning
        .generate_wilaya_request(Uuid::new_v4(), &node.node_key_store)
        .expect("csr generated");
    let signature = root_signer()
        .sign_certificate(&request)
        .expect("root signed request");
    let mut signed = request;
    signed.signature = Some(Ed25519CertificateSignature::try_from(signature).expect("sig wrap"));
    match provisioning
        .finalize_wilaya_provision(&signed, &node.node_key_store, FIXED_NOW)
        .expect("wilaya finalized")
    {
        FinalizeWilayaProvisionResult::Provisioned(cert) => cert,
        FinalizeWilayaProvisionResult::AlreadyProvisioned(_) => {
            panic!("first finalize must provision")
        }
    }
}

/// A fully-provisioned WILAYA node with the whole ContractCatalog fixture:
///
/// units      unit-a "UNIT-A" | unit-b "UNIT-B"
/// suppliers  sup-a1, sup-a2, sup-b1, sup-shared (A+B), sup-orphan (unreferenced)
/// contracts  A: ctr-a1 active, ctr-a2 active, ctr-a3 accepted,
///               ctr-a4 ended, ctr-a5 proposed (excluded), ctr-a6 cancelled (excluded)
///            B: ctr-b1 active
/// allocations + exceptions, global tax policies FY2026/FY2027.
fn provisioned_catalog_node() -> Node {
    let mut node = fresh_node();
    bootstrap_wilaya(&mut node);
    seed_catalog(&node.db);
    node
}

fn seed_catalog(db: &Database) {
    let now = FIXED_NOW;
    let ex = db.executor();

    for (id, code, name) in [
        ("unit-a", "UNIT-A", "Unite A"),
        ("unit-b", "UNIT-B", "Unite B"),
    ] {
        ex.units()
            .upsert_raw_unit(id, code, name, WILAYA, now)
            .expect("seed unit");
    }

    for (pid, name) in [("prod-1", "P1"), ("prod-2", "P2"), ("prod-3", "P3")] {
        ex.products()
            .insert_raw_product(
                &Product {
                    id: pid.to_string(),
                    name: name.to_string(),
                    base_price: 100.0,
                    year: 2026,
                    created_at: chrono::Utc::now(),
                },
                &ProductUnitConfigCodes {
                    purchase_unit: 1,
                    consumption_unit: 1,
                    conversion_factor: 1,
                    tva_classification: 0,
                },
                now,
            )
            .expect("seed product");
    }

    for (sid, name) in [
        ("sup-a1", "Fournisseur A1"),
        ("sup-a2", "Fournisseur A2"),
        ("sup-b1", "Fournisseur B1"),
        ("sup-shared", "Fournisseur Partage"),
        ("sup-orphan", "Fournisseur Orphelin"),
    ] {
        ex.suppliers()
            .insert_supplier(
                sid,
                &CreateSupplierRequest {
                    name: name.to_string(),
                    contact_info: Some(format!("contact {sid}")),
                },
                now,
            )
            .expect("seed supplier");
    }

    for (unit_id, supplier_id) in [
        ("unit-a", "sup-a1"),
        ("unit-a", "sup-a2"),
        ("unit-a", "sup-shared"),
        ("unit-b", "sup-b1"),
        ("unit-b", "sup-shared"),
    ] {
        ex.suppliers()
            .associate_with_unit(unit_id, supplier_id)
            .expect("seed link");
    }

    // ── Contracts / lines / allocations / exceptions ─────────────────────────
    let contracts = db.executor().contracts();
    contracts
        .insert_contract(
            "ctr-a1",
            &CreateContractRequest {
                unit_id: "unit-a".into(),
                supplier_id: "sup-a1".into(),
                fiscal_year: 2026,
                contract_reference: "REF-A-2026".into(),
                notes: None,
            },
            now,
        )
        .expect("ctr-a1");
    contracts
        .insert_contract(
            "ctr-a2",
            &CreateContractRequest {
                unit_id: "unit-a".into(),
                supplier_id: "sup-a2".into(),
                fiscal_year: 2027,
                contract_reference: "REF-A-2027".into(),
                notes: None,
            },
            now,
        )
        .expect("ctr-a2");
    contracts
        .insert_contract(
            "ctr-a3",
            &CreateContractRequest {
                unit_id: "unit-a".into(),
                supplier_id: "sup-shared".into(),
                fiscal_year: 2028,
                contract_reference: "REF-A-2028".into(),
                notes: None,
            },
            now,
        )
        .expect("ctr-a3");
    contracts
        .insert_contract(
            "ctr-a4",
            &CreateContractRequest {
                unit_id: "unit-a".into(),
                supplier_id: "sup-a1".into(),
                fiscal_year: 2029,
                contract_reference: "REF-A-2029".into(),
                notes: None,
            },
            now,
        )
        .expect("ctr-a4");
    contracts
        .insert_contract(
            "ctr-a5",
            &CreateContractRequest {
                unit_id: "unit-a".into(),
                supplier_id: "sup-a1".into(),
                fiscal_year: 2030,
                contract_reference: "REF-A-2030".into(),
                notes: None,
            },
            now,
        )
        .expect("ctr-a5");
    contracts
        .insert_contract(
            "ctr-a6",
            &CreateContractRequest {
                unit_id: "unit-a".into(),
                supplier_id: "sup-a2".into(),
                fiscal_year: 2031,
                contract_reference: "REF-A-CANCEL".into(),
                notes: None,
            },
            now,
        )
        .expect("ctr-a6");
    contracts
        .insert_contract(
            "ctr-b1",
            &CreateContractRequest {
                unit_id: "unit-b".into(),
                supplier_id: "sup-b1".into(),
                fiscal_year: 2026,
                contract_reference: "REF-B-2026".into(),
                notes: None,
            },
            now,
        )
        .expect("ctr-b1");

    // Status transitions.
    let at = "2026-02-01T00:00:00Z";
    contracts
        .update_contract_status("ctr-a1", &["proposed"], "active", "activated_at", at)
        .expect("a1 active");
    contracts
        .update_contract_status("ctr-a2", &["proposed"], "active", "activated_at", at)
        .expect("a2 active");
    contracts
        .update_contract_status("ctr-a3", &["proposed"], "accepted", "accepted_at", at)
        .expect("a3 accepted");
    contracts
        .update_contract_status("ctr-a4", &["proposed"], "accepted", "accepted_at", at)
        .expect("a4 accepted");
    contracts
        .update_contract_status("ctr-a4", &["accepted"], "active", "activated_at", at)
        .expect("a4 active");
    contracts
        .update_contract_status("ctr-a4", &["active"], "ended", "ended_at", at)
        .expect("a4 ended");
    contracts
        .update_contract_status("ctr-a6", &["proposed"], "cancelled", "cancelled_at", at)
        .expect("a6 cancelled");
    contracts
        .update_contract_status("ctr-b1", &["proposed"], "active", "activated_at", at)
        .expect("b1 active");

    // Product lines.
    let add_line = |id: &str, contract_id: &str, product_id: &str, price: f64| {
        contracts
            .insert_contract_product(
                id,
                &grpc_lib::models::AddContractProductRequest {
                    contract_id: contract_id.to_string(),
                    product_id: product_id.to_string(),
                    proposed_price_ht: price,
                    agreed_price_ht: Some(price * 0.9),
                    contracted_quantity: 0.0,
                },
                now,
            )
            .expect("seed line");
    };
    add_line("ctr-prod-a1-1", "ctr-a1", "prod-1", 120.0);
    add_line("ctr-prod-a2-1", "ctr-a2", "prod-2", 80.0);
    add_line("ctr-prod-a2-2", "ctr-a2", "prod-3", 60.0);
    add_line("ctr-prod-a3-1", "ctr-a3", "prod-1", 100.0);
    add_line("ctr-prod-a4-1", "ctr-a4", "prod-2", 90.0);
    add_line("ctr-prod-a5-1", "ctr-a5", "prod-1", 95.0);
    add_line("ctr-prod-a6-1", "ctr-a6", "prod-2", 70.0);
    add_line("ctr-prod-b1-1", "ctr-b1", "prod-1", 110.0);

    // Allocations.
    contracts
        .insert_allocation(
            "alloc-a1-1",
            "ctr-a1",
            "ctr-prod-a1-1",
            "unit-a",
            "prod-1",
            2026,
            100.0,
            now,
        )
        .expect("alloc-a1-1");
    contracts
        .insert_allocation(
            "alloc-a2-1",
            "ctr-a2",
            "ctr-prod-a2-1",
            "unit-a",
            "prod-2",
            2027,
            200.0,
            now,
        )
        .expect("alloc-a2-1");
    contracts
        .insert_allocation(
            "alloc-a2-2",
            "ctr-a2",
            "ctr-prod-a2-2",
            "unit-a",
            "prod-3",
            2027,
            150.0,
            now,
        )
        .expect("alloc-a2-2");
    contracts
        .insert_allocation(
            "alloc-a3-1",
            "ctr-a3",
            "ctr-prod-a3-1",
            "unit-a",
            "prod-1",
            2028,
            50.0,
            now,
        )
        .expect("alloc-a3-1");
    contracts
        .insert_allocation(
            "alloc-a4-1",
            "ctr-a4",
            "ctr-prod-a4-1",
            "unit-a",
            "prod-2",
            2029,
            80.0,
            now,
        )
        .expect("alloc-a4-1");
    contracts
        .insert_allocation(
            "alloc-b1-1",
            "ctr-b1",
            "ctr-prod-b1-1",
            "unit-b",
            "prod-1",
            2026,
            300.0,
            now,
        )
        .expect("alloc-b1-1");
    // Cross-unit allocation (defensive isolation probe): a row whose owning
    // UNIT is B but whose contract chain is owned by A. It must appear in
    // NEITHER artifact (A: row unit differs; B: contract chain excluded).
    contracts
        .insert_allocation(
            "alloc-a1-b",
            "ctr-a1",
            "ctr-prod-a1-1",
            "unit-b",
            "prod-3",
            2026,
            999.0,
            now,
        )
        .expect("alloc-a1-b");

    // Exceptions.
    contracts
        .insert_exception(
            "exc-a1",
            "alloc-a1-1",
            ReleaseReasonCode::SupplierDelay,
            5.0,
            &None,
            "wilaya-admin",
            now,
        )
        .expect("exc-a1");
    contracts
        .insert_exception(
            "exc-b1",
            "alloc-b1-1",
            ReleaseReasonCode::ServiceContinuity,
            12.0,
            &None,
            "wilaya-admin",
            now,
        )
        .expect("exc-b1");

    // Global tax policies (unit-independent).
    for (year, rate) in [(2026, 19.0), (2027, 7.0)] {
        db.executor()
            .fiscal_tax_policy()
            .insert_policy(
                &SetTaxPolicyRequest {
                    fiscal_year: year,
                    tva_rate: rate,
                },
                "wilaya-admin",
                now,
            )
            .expect("seed policy");
    }
}

/// UNIT-scoped export producing nothing before fail-closed resolution; returns
/// the outcome and the on-disk artifacts.
fn unit_export(
    node: &mut Node,
    dir: &TempDir,
    base: &str,
    targets: &[&str],
) -> (
    grpc_lib::application::services::FleetExportOutcome,
    Vec<std::path::PathBuf>,
) {
    let crypto = AgeFileEncryptionProvider::new();
    let requested = dir.path().join(format!("{base}.sync"));
    let outcome = export_contract_catalog_unit_distribution(
        &mut node.db,
        &node.node_key_store,
        &crypto,
        "wilaya-test-node",
        SubjectType::Wilaya,
        &requested,
        &targets.iter().map(|t| t.to_string()).collect::<Vec<_>>(),
    )
    .expect("unit distribution export");
    (outcome.clone(), outcome.artifact_paths)
}

fn read_package(
    path: &std::path::Path,
) -> grpc_lib::application::sync::SyncPackage<
    grpc_lib::application::usecases::exports::types::ContractCatalogExportDataset,
> {
    let crypto = AgeFileEncryptionProvider::new();
    read_contract_catalog_package_from_file(path, &crypto).expect("read package")
}

fn contract_ids(
    rows: &[grpc_lib::application::usecases::exports::types::ContractCatalogContractRow],
) -> HashSet<&str> {
    rows.iter().map(|r| r.contract.id.as_str()).collect()
}

// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn unit_distribution_emits_one_independent_artifact_per_target() {
    let mut node = provisioned_catalog_node();
    let dir = TempDir::new().expect("temp dir");

    let (outcome, paths) = unit_export(&mut node, &dir, "base", &["UNIT-A", "UNIT-B"]);

    assert_eq!(outcome.targets, vec!["UNIT-A", "UNIT-B"]);
    assert_eq!(paths.len(), 2, "one artifact per target");
    assert!(
        paths[0].to_string_lossy().ends_with("base-UNIT-A.sync"),
        "derived artifact name: {}",
        paths[0].display()
    );
    assert!(
        paths[1].to_string_lossy().ends_with("base-UNIT-B.sync"),
        "derived artifact name: {}",
        paths[1].display()
    );
    // record_count = sum of per-target contract counts.
    assert_eq!(
        outcome.record_count,
        read_package(&paths[0]).payload.contracts.len()
            + read_package(&paths[1]).payload.contracts.len()
    );

    let a = read_package(&paths[0]);
    let b = read_package(&paths[1]);
    assert_eq!(
        a.metadata.export_mode,
        Some(PackageExportMode::UnitDistribution),
        "UNIT-A artifact is explicitly UnitDistribution"
    );
    assert_eq!(
        a.metadata.target_node_id.as_deref(),
        Some("UNIT-A"),
        "target code is the exact target_node_id"
    );
    assert_eq!(
        b.metadata.export_mode,
        Some(PackageExportMode::UnitDistribution)
    );
    assert_eq!(b.metadata.target_node_id.as_deref(), Some("UNIT-B"));
    assert_ne!(
        a.metadata.package_id, b.metadata.package_id,
        "each artifact is an independent package"
    );
}

#[test]
fn unit_distribution_scopes_contracts_allocations_and_exceptions() {
    let mut node = provisioned_catalog_node();
    let dir = TempDir::new().expect("temp dir");
    let (_, paths) = unit_export(&mut node, &dir, "base", &["UNIT-A"]);
    let a = read_package(&paths[0]);

    // Contracts: accepted/active/ended for unit-a ONLY.
    let ids = contract_ids(&a.payload.contracts);
    let expected = ["ctr-a1", "ctr-a2", "ctr-a3", "ctr-a4"];
    for id in expected {
        assert!(ids.contains(id), "{id} must be in unit-a artifact");
    }
    for id in ["ctr-a5", "ctr-a6", "ctr-b1"] {
        assert!(
            !ids.contains(id),
            "{id} must NOT be in unit-a artifact (proposed/cancelled/other-unit)"
        );
    }
    for row in &a.payload.contracts {
        assert_eq!(
            row.contract.unit_id, "unit-a",
            "every contract in a unit artifact is owned by the target"
        );
    }

    // Allocations: only unit-a rows riding the target-owned contract chain.
    let mut alloc_ids: Vec<&str> = Vec::new();
    for row in &a.payload.contracts {
        for alloc in &row.allocations {
            assert_eq!(
                alloc.allocation.unit_id, "unit-a",
                "allocation unit filter must match the target"
            );
            alloc_ids.push(alloc.allocation.id.as_str());
        }
        // Contract headers detected on the SQL boundary: no other-unit contract
        // can exist, so alloc-a1-b (row owner = unit-b, chain = ctr-a1) must be
        // absent from A's artifact.
        assert!(
            !row.allocations
                .iter()
                .any(|x| x.allocation.id == "alloc-a1-b"),
            "cross-unit allocation must never leak"
        );
    }
    for id in [
        "alloc-a1-1",
        "alloc-a2-1",
        "alloc-a2-2",
        "alloc-a3-1",
        "alloc-a4-1",
    ] {
        assert!(alloc_ids.contains(&id), "{id} must ride unit-a chain");
    }
    assert!(!alloc_ids.contains(&"alloc-b1-1"));

    // Exceptions: only those of unit-a allocations.
    let mut exc_ids: Vec<&str> = Vec::new();
    for row in &a.payload.contracts {
        for exc in &row.exceptions {
            exc_ids.push(exc.id.as_str());
        }
    }
    assert_eq!(exc_ids, vec!["exc-a1"]);
}

#[test]
fn unit_distribution_restricts_suppliers_and_links() {
    let mut node = provisioned_catalog_node();
    let dir = TempDir::new().expect("temp dir");
    let (_, paths) = unit_export(&mut node, &dir, "base", &["UNIT-A", "UNIT-B"]);

    let a = read_package(&paths[0]);
    let mut a_suppliers: HashSet<&str> =
        a.payload.suppliers.iter().map(|s| s.id.as_str()).collect();
    for id in ["sup-a1", "sup-a2", "sup-shared"] {
        assert!(a_suppliers.remove(id), "{id} referenced by unit-a");
    }
    assert!(
        a_suppliers.is_empty(),
        "unit-a artifact carries ONLY its referenced suppliers: {a_suppliers:?}"
    );
    let a_links: HashSet<(&str, &str)> = a
        .payload
        .unit_supplier_links
        .iter()
        .map(|l| (l.unit_id.as_str(), l.supplier_id.as_str()))
        .collect();
    assert_eq!(
        a_links,
        HashSet::from([
            ("unit-a", "sup-a1"),
            ("unit-a", "sup-a2"),
            ("unit-a", "sup-shared")
        ]),
        "unit-a links only"
    );

    let b = read_package(&paths[1]);
    let mut b_suppliers: HashSet<&str> =
        b.payload.suppliers.iter().map(|s| s.id.as_str()).collect();
    for id in ["sup-b1", "sup-shared"] {
        assert!(b_suppliers.remove(id), "{id} referenced by unit-b");
    }
    assert!(
        b_suppliers.is_empty(),
        "unit-b artifact carries ONLY its referenced suppliers: {b_suppliers:?}"
    );
    let b_links: HashSet<(&str, &str)> = b
        .payload
        .unit_supplier_links
        .iter()
        .map(|l| (l.unit_id.as_str(), l.supplier_id.as_str()))
        .collect();
    assert_eq!(
        b_links,
        HashSet::from([("unit-b", "sup-b1"), ("unit-b", "sup-shared")]),
        "unit-b links only"
    );

    // sup-orphan is referenced by no unit relationship — it must not ship.
    assert!(!a_suppliers.contains("sup-orphan") && !b_suppliers.contains("sup-orphan"));
}

#[test]
fn tax_policies_are_global_in_every_artifact() {
    let mut node = provisioned_catalog_node();
    let dir = TempDir::new().expect("temp dir");
    let (_, paths) = unit_export(&mut node, &dir, "base", &["UNIT-A", "UNIT-B"]);

    let a = read_package(&paths[0]);
    let b = read_package(&paths[1]);
    let years_a: Vec<i32> = a
        .payload
        .tax_policies
        .iter()
        .map(|p| p.fiscal_year)
        .collect();
    let years_b: Vec<i32> = b
        .payload
        .tax_policies
        .iter()
        .map(|p| p.fiscal_year)
        .collect();
    assert_eq!(
        years_a,
        vec![2027, 2026],
        "global policies ride every artifact"
    );
    assert_eq!(years_b, vec![2027, 2026]);
    assert_eq!(
        serde_json::to_value(&a.payload.tax_policies).expect("a policies"),
        serde_json::to_value(&b.payload.tax_policies).expect("b policies")
    );
}

#[test]
fn fleet_restore_artifacts_are_unscoped_and_explicitly_labeled() {
    let mut node = provisioned_catalog_node();
    let dir = TempDir::new().expect("temp dir");
    let crypto = AgeFileEncryptionProvider::new();
    let requested = dir.path().join("fleet.sync");

    let outcome = export_contract_catalog_fleet(
        &mut node.db,
        &node.node_key_store,
        &crypto,
        "wilaya-test-node",
        SubjectType::Wilaya,
        &requested,
    )
    .expect("fleet export");
    assert_eq!(
        outcome.targets,
        vec!["UNIT-A", "UNIT-B"],
        "fleet targets = all registered units, order-stable"
    );
    assert_eq!(outcome.artifact_paths.len(), 2);

    let a = read_contract_catalog_package_from_file(&outcome.artifact_paths[0], &crypto)
        .expect("read fleet artifact A");
    let b = read_contract_catalog_package_from_file(&outcome.artifact_paths[1], &crypto)
        .expect("read fleet artifact B");

    // Fleet is explicitly FleetRestore with no target — never inferred.
    assert_eq!(
        a.metadata.export_mode,
        Some(PackageExportMode::FleetRestore)
    );
    assert_eq!(a.metadata.target_node_id, None);
    assert_eq!(
        b.metadata.export_mode,
        Some(PackageExportMode::FleetRestore)
    );
    assert_eq!(b.metadata.target_node_id, None);

    // Unscoped: BOTH artifacts carry ALL units and ALL statuses (proposed +
    // cancelled included) — pre-C3 fleet semantics preserved.
    for pkg in [&a, &b] {
        let ids = contract_ids(&pkg.payload.contracts);
        for ctr in [
            "ctr-a1", "ctr-a2", "ctr-a3", "ctr-a4", "ctr-a5", "ctr-a6", "ctr-b1",
        ] {
            assert!(
                ids.contains(ctr),
                "fleet {ctr} must be present (all statuses)"
            );
        }
        let mut suppliers: HashSet<&str> = pkg
            .payload
            .suppliers
            .iter()
            .map(|s| s.id.as_str())
            .collect();
        for s in ["sup-a1", "sup-a2", "sup-b1", "sup-shared", "sup-orphan"] {
            assert!(suppliers.remove(s), "fleet {s} must be present (unscoped)");
        }
        assert!(suppliers.is_empty(), "no unreferenced suppliers dropped");
    }
    // Same dataset shape on every fleet artifact.
    assert_eq!(
        serde_json::to_value(&a.payload).expect("A value"),
        serde_json::to_value(&b.payload).expect("B value")
    );
    assert_eq!(outcome.record_count, a.payload.contracts.len());
}

#[test]
fn unit_artifacts_verify_v2_signature_against_issuer() {
    let mut node = provisioned_catalog_node();
    let dir = TempDir::new().expect("temp dir");
    let (_, paths) = unit_export(&mut node, &dir, "base", &["UNIT-A", "UNIT-B"]);

    for path in &paths {
        let pkg = read_package(path);
        assert!(
            pkg.metadata.signature_version.is_some() && pkg.metadata.signature.is_some(),
            "V2 Ed25519 signed envelope"
        );
        assert_eq!(
            pkg.metadata.export_mode,
            Some(PackageExportMode::UnitDistribution),
            "mode + target are authenticated inside the signed envelope"
        );
        SyncPackageIdentityVerificationService::verify_v2_signature(make_executor(&node.db), &pkg)
            .expect("unit artifact verifies against issuer certificate");
    }
}

#[test]
fn empty_unit_artifact_is_legitimate() {
    let mut node = provisioned_catalog_node();
    // An extra unit with NO contracts/links/suppliers.
    node.db
        .executor()
        .units()
        .upsert_raw_unit("unit-empty", "UNIT-EMPTY", "Unite Vide", WILAYA, FIXED_NOW)
        .expect("seed empty unit");
    let dir = TempDir::new().expect("temp dir");

    let (_, paths) = unit_export(&mut node, &dir, "base", &["UNIT-EMPTY"]);
    let pkg = read_package(&paths[0]);
    assert_eq!(
        pkg.metadata.export_mode,
        Some(PackageExportMode::UnitDistribution),
        "an empty unit scroll is still a legitimate UnitDistribution artifact"
    );
    assert_eq!(pkg.metadata.target_node_id.as_deref(), Some("UNIT-EMPTY"));
    assert!(pkg.payload.contracts.is_empty());
    assert!(pkg.payload.unit_supplier_links.is_empty());
    assert!(pkg.payload.suppliers.is_empty());
    // Global tax policies still ride.
    assert_eq!(pkg.payload.tax_policies.len(), 2);
}

#[test]
fn deterministic_repeat_export_identical_dataset_distinct_package() {
    let mut node = provisioned_catalog_node();
    let dir = TempDir::new().expect("temp dir");
    let crypto = AgeFileEncryptionProvider::new();

    let first = dir.path().join("run1.sync");
    export_contract_catalog_unit_distribution(
        &mut node.db,
        &node.node_key_store,
        &crypto,
        "wilaya-test-node",
        SubjectType::Wilaya,
        &first,
        &["UNIT-A".to_string()],
    )
    .expect("first run");
    let second = dir.path().join("run2.sync");
    export_contract_catalog_unit_distribution(
        &mut node.db,
        &node.node_key_store,
        &crypto,
        "wilaya-test-node",
        SubjectType::Wilaya,
        &second,
        &["UNIT-A".to_string()],
    )
    .expect("second run");

    let p1 = read_package(&first);
    let p2 = read_package(&second);
    // Dataset content is perfectly reproducible.
    assert_eq!(
        serde_json::to_value(&p1.payload).expect("p1 value"),
        serde_json::to_value(&p2.payload).expect("p2 value"),
        "identical inputs + persisted state → identical dataset projection"
    );
    // Every export remains a distinct package instance.
    assert_ne!(
        p1.metadata.package_id, p2.metadata.package_id,
        "repeat export must mint a fresh package_id"
    );
}

#[test]
fn missing_target_fails_closed_before_any_artifact() {
    let mut node = provisioned_catalog_node();
    let dir = TempDir::new().expect("temp dir");
    let crypto = AgeFileEncryptionProvider::new();
    let requested = dir.path().join("partial.sync");

    let err = export_contract_catalog_unit_distribution(
        &mut node.db,
        &node.node_key_store,
        &crypto,
        "wilaya-test-node",
        SubjectType::Wilaya,
        &requested,
        &["UNIT-A".to_string(), "UNIT-MISSING".to_string()],
    )
    .expect_err("unknown target must abort the whole batch");
    let err = format!("{err}");
    assert!(
        err.contains("رفض مغلق") || err.contains("غير موجودة"),
        "fail-closed message, got: {err}"
    );
    // Phase-1 resolution precedes emission: NO artifact may exist.
    assert!(
        !requested.exists() && !dir.path().join("partial-UNIT-A.sync").exists(),
        "no partial artifact set emitted"
    );
}

#[test]
fn empty_targets_are_rejected() {
    let mut node = provisioned_catalog_node();
    let dir = TempDir::new().expect("temp dir");
    let crypto = AgeFileEncryptionProvider::new();
    let requested = dir.path().join("empty.sync");

    let err = export_contract_catalog_unit_distribution(
        &mut node.db,
        &node.node_key_store,
        &crypto,
        "wilaya-test-node",
        SubjectType::Wilaya,
        &requested,
        &[],
    )
    .expect_err("an empty target list must be rejected, never inferred");
    assert!(!format!("{err}").is_empty());
    assert!(!requested.exists());
}

#[test]
fn malformed_other_unit_never_invalidates_target_artifact() {
    let mut node = provisioned_catalog_node();
    // Make UNIT-B's data malformed (a selected contract with an empty
    // reference), which the producer validator must reject for B.
    seed_malformed_unit_b_contract(&node.db);
    let dir = TempDir::new().expect("temp dir");
    let crypto = AgeFileEncryptionProvider::new();
    let target = dir.path().join("a-only.sync");

    // Critically: UNIT-A export must SUCCEED — B's invalid rows are never part
    // of the dataset selected for A (construction precedes validation).
    export_contract_catalog_unit_distribution(
        &mut node.db,
        &node.node_key_store,
        &crypto,
        "wilaya-test-node",
        SubjectType::Wilaya,
        &target,
        &["UNIT-A".to_string()],
    )
    .expect("unit-a export unaffected by unit-b defects");
    let a = read_package(&target);
    assert!(
        !contract_ids(&a.payload.contracts).contains("ctr-bad"),
        "unit-b's malformed contract never appears in unit-a artifact"
    );

    // Whereas UNIT-B export fails closed on its own selected dataset.
    let b_target = dir.path().join("b-only.sync");
    let err = export_contract_catalog_unit_distribution(
        &mut node.db,
        &node.node_key_store,
        &crypto,
        "wilaya-test-node",
        SubjectType::Wilaya,
        &b_target,
        &["UNIT-B".to_string()],
    )
    .expect_err("unit-b's own dataset must be rejected");
    assert!(
        format!("{err}").contains("مرجع"),
        "empty reference flagged, got: {err}"
    );
}

#[test]
fn batch_aborts_closed_when_any_target_dataset_is_invalid() {
    let mut node = provisioned_catalog_node();
    seed_malformed_unit_b_contract(&node.db);
    let dir = TempDir::new().expect("temp dir");
    let crypto = AgeFileEncryptionProvider::new();
    let requested = dir.path().join("batch.sync");

    let err = export_contract_catalog_unit_distribution(
        &mut node.db,
        &node.node_key_store,
        &crypto,
        "wilaya-test-node",
        SubjectType::Wilaya,
        &requested,
        &["UNIT-A".to_string(), "UNIT-B".to_string()],
    )
    .expect_err("malformed B aborts the whole batch");
    assert!(!format!("{err}").is_empty());
    // Phase 1 (all datasets built + validated) precedes phase 2 (emission);
    // therefore NOTHING is written — not even the healthy A artifact.
    assert!(
        !requested.exists() && !dir.path().join("batch-UNIT-A.sync").exists(),
        "batch fail-loud leaves no artifacts behind"
    );
}

#[test]
fn snapshot_agrees_with_artifact_mode_and_target() {
    let mut node = provisioned_catalog_node();
    let dir = TempDir::new().expect("temp dir");
    let crypto = AgeFileEncryptionProvider::new();

    // Export one unit artifact and confirm its authenticated metadata.
    let unit_path = dir.path().join("snap.sync");
    export_contract_catalog_unit_distribution(
        &mut node.db,
        &node.node_key_store,
        &crypto,
        "wilaya-test-node",
        SubjectType::Wilaya,
        &unit_path,
        &["UNIT-A".to_string()],
    )
    .expect("unit export");
    let pkg = read_package(&unit_path);
    assert_eq!(
        pkg.metadata.export_mode,
        Some(PackageExportMode::UnitDistribution)
    );
    assert_eq!(pkg.metadata.target_node_id.as_deref(), Some("UNIT-A"));

    // Mirrors the command wiring: the fleet snapshot records
    // ("fleet_restore", None); the unit command records per target
    // ("unit_distribution", Some(code)). Persist + read back.
    let signing_key_id = current_wilaya_signing_key_id(&node.db, &node.node_key_store);
    let unit_hash = Uuid::new_v4().to_string();
    record_export_with_reproducibility(
        make_executor(&node.db),
        signing_key_id.clone(),
        ExportReproducibilityContext {
            export_hash: unit_hash.clone(),
            fiscal_year: 2026,
            generated_by: "admin".into(),
            movement_count: 0,
            report_count: 0,
            inventory_total_value: 0.0,
            export_reason: "contract_catalog_sync_package".into(),
            export_mode: Some("unit_distribution".into()),
            target_node_id: Some("UNIT-A".into()),
        },
    )
    .expect("record unit snapshot");
    let fleet_hash = Uuid::new_v4().to_string();
    record_export_with_reproducibility(
        make_executor(&node.db),
        signing_key_id,
        ExportReproducibilityContext {
            export_hash: fleet_hash.clone(),
            fiscal_year: 2026,
            generated_by: "admin".into(),
            movement_count: 0,
            report_count: 0,
            inventory_total_value: 0.0,
            export_reason: "contract_catalog_sync_package".into(),
            export_mode: Some("fleet_restore".into()),
            target_node_id: None,
        },
    )
    .expect("record fleet snapshot");

    let unit_snap = node
        .db
        .executor()
        .fiscal_snapshots()
        .get_export_snapshot_by_hash(&unit_hash)
        .expect("read unit snapshot")
        .expect("present")
        .export_mode;
    let unit_target = node
        .db
        .executor()
        .fiscal_snapshots()
        .get_export_snapshot_by_hash(&unit_hash)
        .expect("read unit snapshot")
        .expect("present")
        .target_node_id;
    assert_eq!(unit_snap.as_deref(), Some("unit_distribution"));
    assert_eq!(unit_target.as_deref(), Some("UNIT-A"));

    let fleet_snap = node
        .db
        .executor()
        .fiscal_snapshots()
        .get_export_snapshot_by_hash(&fleet_hash)
        .expect("read fleet snapshot")
        .expect("present")
        .export_mode;
    let fleet_target = node
        .db
        .executor()
        .fiscal_snapshots()
        .get_export_snapshot_by_hash(&fleet_hash)
        .expect("read fleet snapshot")
        .expect("present")
        .target_node_id;
    assert_eq!(fleet_snap.as_deref(), Some("fleet_restore"));
    assert_eq!(fleet_target, None);
}

#[test]
fn list_contracts_filters_are_parameter_bound_not_interpolated() {
    // ADR-0059 deferred SQL hardening: `list_contracts` unit_id / supplier_id /
    // fiscal_year filters use `(?N IS NULL OR col = ?N)` parameters. A value
    // carrying a quoted SQL fragment must be treated as opaque DATA — never
    // evaluated — so no row leaks and nothing errors.
    let node = provisioned_catalog_node();
    let ex = node.db.executor();
    let repo = ex.contracts();

    let injection = "' OR 1=1 --";
    let leaked = repo
        .list_contracts(Some(injection), None, None)
        .expect("parameterized unit_id filter must not error");
    assert!(
        leaked.is_empty(),
        "quoted-fragment unit_id must not match any contract (got {})",
        leaked.len()
    );
    let leaked_supplier = repo
        .list_contracts(None, Some(injection), None)
        .expect("parameterized supplier_id filter must not error");
    assert!(
        leaked_supplier.is_empty(),
        "quoted-fragment supplier_id must not match any contract (got {})",
        leaked_supplier.len()
    );

    // Exact-match filters still resolve to the intended contract only.
    let filtered = repo
        .list_contracts(Some("unit-a"), Some("sup-a1"), Some(2026))
        .expect("exact filters");
    let ids: Vec<&str> = filtered.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(
        ids,
        vec!["ctr-a1"],
        "exact filters resolve to the owned row only"
    );
}

/// Insert an ACTIVE contract for UNIT-B whose `contract_reference` is empty —
/// structurally invalid per the producer validator (selected-dataset rejects it
/// for B only).
fn seed_malformed_unit_b_contract(db: &Database) {
    let now = FIXED_NOW;
    let contracts = db.executor().contracts();
    contracts
        .insert_contract(
            "ctr-bad",
            &CreateContractRequest {
                unit_id: "unit-b".into(),
                supplier_id: "sup-b1".into(),
                fiscal_year: 2028,
                contract_reference: String::new(),
                notes: None,
            },
            now,
        )
        .expect("seed malformed B contract");
    contracts
        .update_contract_status("ctr-bad", &["proposed"], "active", "activated_at", now)
        .expect("activate malformed B contract");
    contracts
        .insert_contract_product(
            "ctr-prod-bad-1",
            &grpc_lib::models::AddContractProductRequest {
                contract_id: "ctr-bad".into(),
                product_id: "prod-1".into(),
                proposed_price_ht: 42.0,
                agreed_price_ht: Some(40.0),
                contracted_quantity: 0.0,
            },
            now,
        )
        .expect("malformed B contract product line");
}
