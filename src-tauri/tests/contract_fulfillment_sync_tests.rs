//! ADR-0061 — allocation-level cumulative fulfillment state sync
//! (`contract_fulfillment`, UNIT → WILAYA).
//!
//! The contract these tests pin down:
//!
//! - the fact is an **allocation-level absolute snapshot**, not an event and
//!   never a delta;
//! - the package identity is **derived from the content**, so re-exporting an
//!   unchanged state set is the same package;
//! - application is a **guarded monotone SET**, so duplicate and out-of-order
//!   delivery converge to the same state as exactly-once in-order delivery;
//! - every fact is validated before any mutation and one invalid fact rejects
//!   the WHOLE package;
//! - the unit triple and the fiscal year are cross-checks only — they never
//!   filter which allocations may converge, so fulfillment that advanced after
//!   a fiscal year closed is never silently dropped.

use chrono::Utc;

use grpc_lib::application::sync::{
    validate_contract_fulfillment_package_for_import, SyncPackage, SyncPackageMetadata,
    SYNC_PACKAGE_SCHEMA_VERSION,
};
use grpc_lib::application::usecases::exports::types::{
    FulfillmentFact, FulfillmentFactExportDataset, FULFILLMENT_FACT_VERSION,
};
use grpc_lib::application::usecases::sync::export_contract_fulfillment_package::build_dataset;
use grpc_lib::application::usecases::sync::import_contract_fulfillment_package::{
    execute as apply_contract_fulfillment_package, ImportContractFulfillmentPackageInput,
    ImportContractFulfillmentPackageOutcome, CONTRACT_FULFILLMENT_PACKAGE_KIND,
};
use grpc_lib::db::{ConnectionFactory, Database};
use grpc_lib::domain::identity::SIGNATURE_VERSION_ED25519;
use grpc_lib::errors::{AppError, BusinessLogicError, ValidationError};
use grpc_lib::infrastructure::db::sync_import::SqliteImportedPackageRegistry;
use grpc_lib::infrastructure::sync::packages::canonical_json::canonical_bytes_for_integrity;
use grpc_lib::infrastructure::sync::packages::content_package_id::derive_content_package_id;
use grpc_lib::infrastructure::sync::packages::integrity::{PackageHasher, Sha256PackageHasher};
use grpc_lib::infrastructure::sync::packages::SerdeJsonSyncPackageDeserializer;
use grpc_lib::models::{
    AddContractProductRequest, CreateContractRequest, CreateSupplierRequest, NodeType, Product,
    ProductUnitConfigCodes, WilayaNodeConfiguration,
};
use grpc_lib::repositories::{
    ContractRepository, ProductRepository, SettingsRepository, SupplierRepository, UnitRepository,
};

const WILAYA: &str = "16";
const UNIT: &str = "unit-a";
const STAMP: &str = "2026-01-01T00:00:00Z";

// ─── Fixtures ────────────────────────────────────────────────────────────────

fn fact(allocation_id: &str, fulfilled: f64, fiscal_year: i32) -> FulfillmentFact {
    FulfillmentFact {
        allocation_id: allocation_id.to_string(),
        fulfilled_quantity: fulfilled,
        fiscal_year,
        purchase_unit: Some(1),
        conversion_factor: Some(1),
    }
}

fn dataset(facts: Vec<FulfillmentFact>) -> FulfillmentFactExportDataset {
    FulfillmentFactExportDataset {
        fact_version: FULFILLMENT_FACT_VERSION,
        facts,
    }
}

/// Package whose `package_id` is the honest content-derived identity.
fn package(
    payload: FulfillmentFactExportDataset,
    source: &str,
) -> SyncPackage<FulfillmentFactExportDataset> {
    let package_id = derive_content_package_id(&payload).expect("derive content id");
    SyncPackage {
        metadata: SyncPackageMetadata {
            schema_version: SYNC_PACKAGE_SCHEMA_VERSION,
            created_at: Utc::now(),
            source_node_id: source.to_string(),
            issuer_identity_id: None,
            package_id,
            signature_version: None,
            signing_key_id: None,
            integrity_hash: None,
            signature: None,
            export_mode: None,
            target_node_id: None,
        },
        payload,
    }
}

fn apply_package(
    db: &mut Database,
    package: &SyncPackage<FulfillmentFactExportDataset>,
) -> Result<ImportContractFulfillmentPackageOutcome, AppError> {
    let source = package.metadata.source_node_id.trim().to_string();
    db.with_transaction(|tx| {
        let reg = SqliteImportedPackageRegistry::new(
            tx,
            CONTRACT_FULFILLMENT_PACKAGE_KIND,
            Some(&source),
            "admin",
            None,
        );
        apply_contract_fulfillment_package(
            tx,
            &reg,
            ImportContractFulfillmentPackageInput {
                package: package.clone(),
                unit_id: UNIT.to_string(),
                importer_wilaya_code: WILAYA.to_string(),
                imported_by: "admin".to_string(),
            },
        )
    })
}

fn fulfilled_of(db: &Database, allocation_id: &str) -> f64 {
    ContractRepository::new(db.executor())
        .get_allocation(allocation_id)
        .expect("read allocation")
        .expect("allocation exists")
        .fulfilled_quantity
}

fn seed(db: &mut Database) {
    SettingsRepository::new(db.executor())
        .configure_wilaya(&WilayaNodeConfiguration {
            node_type: NodeType::Wilaya,
            wilaya_code: Some(WILAYA.into()),
            wilaya_name: Some("Alger".into()),
        })
        .expect("configure wilaya");
    UnitRepository::new(db.executor())
        .upsert_raw_unit(UNIT, "A1", "unit a", WILAYA, STAMP)
        .expect("seed unit");
    SupplierRepository::new(db.executor())
        .insert_supplier(
            "sup-1",
            &CreateSupplierRequest {
                name: "Fournisseur".into(),
                contact_info: None,
            },
            STAMP,
        )
        .expect("seed supplier");

    let now = "2026-01-01T00:00:00Z".to_string();
    let products = ProductRepository::new(db.executor());
    // Two products so the fixture can hold two independent allocations — the
    // schema allows one allocation per (unit, product, fiscal year).
    for (pid, name) in [("prod-1", "P1"), ("prod-2", "P2")] {
        products
            .insert_raw_product(
                &Product {
                    id: pid.to_string(),
                    name: name.to_string(),
                    base_price: 100.0,
                    year: 2026,
                    created_at: Utc::now(),
                },
                &ProductUnitConfigCodes {
                    purchase_unit: 1,
                    consumption_unit: 1,
                    conversion_factor: 1,
                    tva_classification: 0,
                },
                &now,
            )
            .expect("seed product");
    }

    let contracts = ContractRepository::new(db.executor());
    contracts
        .insert_contract(
            "ctr-1",
            &CreateContractRequest {
                unit_id: UNIT.into(),
                supplier_id: "sup-1".into(),
                fiscal_year: 2026,
                contract_reference: "REF-1".into(),
                notes: None,
            },
            STAMP,
        )
        .expect("seed contract");
    for (cp_id, pid) in [("cp-1", "prod-1"), ("cp-2", "prod-2")] {
        contracts
            .insert_contract_product(
                cp_id,
                &AddContractProductRequest {
                    contract_id: "ctr-1".into(),
                    product_id: pid.into(),
                    proposed_price_ht: 100.0,
                    agreed_price_ht: Some(100.0),
                    contracted_quantity: 100.0,
                },
                STAMP,
            )
            .expect("seed contract product");
    }
    // purchase_unit / conversion_factor are the contract product's authoritative
    // unit triple; set them explicitly so the cross-check has something to read.
    db.executor()
        .execute(
            "UPDATE contract_products SET purchase_unit = 1, consumption_unit = 1, conversion_factor = 1",
            [],
        )
        .expect("seed unit triple");
    contracts
        .insert_allocation(
            "alloc-1", "ctr-1", "cp-1", UNIT, "prod-1", 2026, 100.0, STAMP,
        )
        .expect("seed allocation");
    contracts
        .insert_allocation(
            "alloc-2", "ctr-1", "cp-2", UNIT, "prod-2", 2026, 100.0, STAMP,
        )
        .expect("seed second allocation");
}

// ─── Producer: deterministic, content-derived identity ───────────────────────

#[test]
fn producer_refuses_to_build_an_unimportable_empty_package() {
    let mut db = ConnectionFactory::new_for_test().expect("db");
    seed(&mut db);

    // The destination rejects an empty fact set, so the producer must refuse it
    // too — otherwise it signs, encrypts and persists an artifact that can
    // never be imported.
    let err = build_dataset(db.executor()).expect_err("empty dataset must be refused");
    assert!(matches!(
        err,
        AppError::Validation(ValidationError::InvalidFormat { ref field, .. }) if field == "facts"
    ));
}

#[test]
fn producer_identity_is_content_derived_and_deterministic() {
    let mut db = ConnectionFactory::new_for_test().expect("db");
    seed(&mut db);

    ContractRepository::new(db.executor())
        .try_increment_fulfilled("alloc-1", 12.5)
        .expect("local fulfillment");

    let (before, before_id) = build_dataset(db.executor()).expect("build dataset");
    assert_eq!(
        before_id,
        derive_content_package_id(&before).expect("derive"),
        "producer id must be the content hash"
    );

    ContractRepository::new(db.executor())
        .try_increment_fulfilled("alloc-1", 2.5)
        .expect("local fulfillment");

    let (after, after_id) = build_dataset(db.executor()).expect("build dataset");
    assert_eq!(after.facts.len(), 1);
    assert_eq!(after.facts[0].allocation_id, "alloc-1");
    assert_eq!(after.facts[0].fulfilled_quantity, 15.0);
    assert_ne!(
        after_id, before_id,
        "a state change must change the identity"
    );

    // Re-reading the SAME state yields the SAME identity — this is what makes a
    // repeat export recognisable as a duplicate instead of a new fact stream.
    let (_, again_id) = build_dataset(db.executor()).expect("build dataset");
    assert_eq!(after_id, again_id);
}

#[test]
fn producer_omits_allocations_without_fulfillment() {
    let mut db = ConnectionFactory::new_for_test().expect("db");
    seed(&mut db);

    ContractRepository::new(db.executor())
        .try_increment_fulfilled("alloc-1", 1.0)
        .expect("local fulfillment");

    let (payload, _) = build_dataset(db.executor()).expect("build dataset");
    assert_eq!(payload.facts.len(), 1);
    assert!(
        payload.facts.iter().all(|f| f.fulfilled_quantity > 0.0),
        "only allocations with fulfillment are exported"
    );
}

// ─── Idempotency layer 1+2: version gate and content identity ────────────────

#[test]
fn unknown_fact_version_is_rejected() {
    let mut payload = dataset(vec![fact("alloc-1", 10.0, 2026)]);
    payload.fact_version = FULFILLMENT_FACT_VERSION + 1;
    let err = validate_contract_fulfillment_package_for_import(&package(payload, UNIT))
        .expect_err("unknown version must be rejected");
    assert!(matches!(
        err,
        AppError::Validation(ValidationError::InvalidFormat { ref field, .. }) if field == "fact_version"
    ));
}

#[test]
fn package_id_must_describe_its_own_content() {
    let mut pkg = package(dataset(vec![fact("alloc-1", 10.0, 2026)]), UNIT);
    // Same identity, tampered payload: the identity contract is what catches it.
    pkg.payload.facts[0].fulfilled_quantity = 999.0;
    let err = validate_contract_fulfillment_package_for_import(&pkg)
        .expect_err("a package_id that does not match its content must be rejected");
    assert!(matches!(
        err,
        AppError::Validation(ValidationError::InvalidFormat { ref field, .. }) if field == "package_id"
    ));
}

#[test]
fn empty_fact_set_is_rejected() {
    let err = validate_contract_fulfillment_package_for_import(&package(dataset(vec![]), UNIT))
        .expect_err("an empty package carries no state and must be rejected");
    assert!(matches!(
        err,
        AppError::Validation(ValidationError::InvalidFormat { ref field, .. }) if field == "facts"
    ));
}

// ─── Reader layer: version + content identity gate before any DB access ─────

/// Serialize a package the way a producer would: canonical integrity hash over
/// the hash-less body, then a V2 signature marker. The Ed25519 signature itself
/// is verified by the identity service in the import pipeline, not by the
/// deserializer — this exercises the reader's own gates only.
fn sealed_plaintext(pkg: &SyncPackage<FulfillmentFactExportDataset>) -> Vec<u8> {
    let mut sealed = pkg.clone();
    sealed.metadata.signature_version = Some(SIGNATURE_VERSION_ED25519);
    sealed.metadata.signature = Some("test-signature".into());
    let value = serde_json::to_value(&sealed).expect("to_value");
    let canonical = canonical_bytes_for_integrity(&value).expect("canonical");
    sealed.metadata.integrity_hash = Some(Sha256PackageHasher.hash(&canonical).expect("hash"));
    serde_json::to_vec(&sealed).expect("serialize")
}

fn read_through_deserializer(
    plaintext: &[u8],
) -> Result<SyncPackage<FulfillmentFactExportDataset>, AppError> {
    SerdeJsonSyncPackageDeserializer::contract_fulfillment_from_reader(std::io::Cursor::new(
        plaintext,
    ))
}

#[test]
fn reader_rejects_an_unknown_fact_version() {
    let mut payload = dataset(vec![fact("alloc-1", 10.0, 2026)]);
    payload.fact_version = FULFILLMENT_FACT_VERSION + 1;
    // A package that declares an unsupported fact version is not importable at
    // all, so the honest content identity of the tampered payload is used.
    let mut pkg = package(dataset(vec![fact("alloc-1", 10.0, 2026)]), UNIT);
    pkg.payload = payload;

    let err = read_through_deserializer(&sealed_plaintext(&pkg))
        .expect_err("an unknown fact version must be rejected at the reader");
    assert!(matches!(
        err,
        AppError::Validation(ValidationError::InvalidFormat { ref field, .. }) if field == "fact_version"
    ));
}

#[test]
fn reader_rejects_a_package_id_that_does_not_match_its_content() {
    let mut pkg = package(dataset(vec![fact("alloc-1", 10.0, 2026)]), UNIT);
    // Tamper AFTER the identity was honestly derived, then re-seal so the
    // integrity and signature gates pass and only the content gate can catch it.
    pkg.payload.facts[0].fulfilled_quantity = 999.0;

    let err = read_through_deserializer(&sealed_plaintext(&pkg))
        .expect_err("a package_id that does not describe its content must be rejected");
    assert!(matches!(
        err,
        AppError::Validation(ValidationError::InvalidFormat { ref field, .. }) if field == "package_id"
    ));
}

#[test]
fn reader_accepts_a_well_formed_package() {
    let pkg = package(dataset(vec![fact("alloc-1", 10.0, 2026)]), UNIT);
    let read = read_through_deserializer(&sealed_plaintext(&pkg))
        .expect("a well-formed package passes the reader gates");
    assert_eq!(read.payload.facts[0].fulfilled_quantity, 10.0);
    assert_eq!(read.metadata.package_id, pkg.metadata.package_id);
}

// ─── Idempotency layer 3: monotone convergence ───────────────────────────────

#[test]
fn newer_absolute_state_is_applied() {
    let mut db = ConnectionFactory::new_for_test().expect("db");
    seed(&mut db);

    let pkg = package(
        dataset(vec![
            fact("alloc-1", 30.0, 2026),
            fact("alloc-2", 5.0, 2026),
        ]),
        UNIT,
    );
    let outcome = apply_package(&mut db, &pkg).expect("apply");
    assert_eq!(outcome.applied_count, 2);
    assert_eq!(outcome.already_satisfied_count, 0);
    assert_eq!(fulfilled_of(&db, "alloc-1"), 30.0);
    assert_eq!(fulfilled_of(&db, "alloc-2"), 5.0);
}

#[test]
fn replayed_package_id_is_rejected() {
    let mut db = ConnectionFactory::new_for_test().expect("db");
    seed(&mut db);

    let pkg = package(dataset(vec![fact("alloc-1", 30.0, 2026)]), UNIT);
    apply_package(&mut db, &pkg).expect("first import");
    let err = apply_package(&mut db, &pkg).expect_err("replay must fail");
    assert!(matches!(
        err,
        AppError::BusinessLogic(BusinessLogicError::DuplicateSyncPackage { .. })
    ));
}

#[test]
fn identical_state_in_a_new_package_is_a_verified_no_op() {
    let mut db = ConnectionFactory::new_for_test().expect("db");
    seed(&mut db);

    // Same content, different package identity is impossible by construction, so
    // the convergence case is exercised by advancing a SECOND allocation: the
    // already-converged allocation is re-sent unchanged in the newer package.
    let first = package(dataset(vec![fact("alloc-1", 30.0, 2026)]), UNIT);
    apply_package(&mut db, &first).expect("first import");

    let second = package(
        dataset(vec![
            fact("alloc-1", 30.0, 2026),
            fact("alloc-2", 7.0, 2026),
        ]),
        UNIT,
    );
    let outcome = apply_package(&mut db, &second).expect("second import");
    assert_eq!(outcome.applied_count, 1, "only the newer fact is applied");
    assert_eq!(
        outcome.already_satisfied_count, 1,
        "the equal fact is a no-op"
    );
    assert_eq!(fulfilled_of(&db, "alloc-1"), 30.0);
    assert_eq!(fulfilled_of(&db, "alloc-2"), 7.0);
}

#[test]
fn stale_older_state_never_decreases_local_state() {
    let mut db = ConnectionFactory::new_for_test().expect("db");
    seed(&mut db);

    let newer = package(dataset(vec![fact("alloc-1", 40.0, 2026)]), UNIT);
    apply_package(&mut db, &newer).expect("apply newer");

    let older = package(dataset(vec![fact("alloc-1", 10.0, 2026)]), UNIT);
    let outcome = apply_package(&mut db, &older).expect("stale package is a no-op");
    assert_eq!(outcome.applied_count, 0);
    assert_eq!(outcome.already_satisfied_count, 1);
    assert_eq!(fulfilled_of(&db, "alloc-1"), 40.0, "state must not regress");
}

#[test]
fn delivery_order_does_not_change_the_converged_state() {
    let mut ascending = ConnectionFactory::new_for_test().expect("db");
    let mut descending = ConnectionFactory::new_for_test().expect("db");
    seed(&mut ascending);
    seed(&mut descending);

    for (db, states) in [
        (&mut ascending, vec![10.0_f64, 25.0, 40.0]),
        (&mut descending, vec![40.0_f64, 25.0, 10.0]),
    ] {
        for state in states {
            let pkg = package(dataset(vec![fact("alloc-1", state, 2026)]), UNIT);
            apply_package(db, &pkg).expect("converge");
        }
    }

    assert_eq!(fulfilled_of(&ascending, "alloc-1"), 40.0);
    assert_eq!(fulfilled_of(&descending, "alloc-1"), 40.0);
}

#[test]
fn duplicate_fact_in_one_package_is_rejected() {
    let mut db = ConnectionFactory::new_for_test().expect("db");
    seed(&mut db);

    let pkg = package(
        dataset(vec![
            fact("alloc-1", 10.0, 2026),
            fact("alloc-1", 20.0, 2026),
        ]),
        UNIT,
    );
    let err = apply_package(&mut db, &pkg).expect_err("ambiguous identity must fail");
    assert!(matches!(
        err,
        AppError::Validation(ValidationError::InvalidFormat { ref field, .. }) if field == "allocation_id"
    ));
    assert_eq!(fulfilled_of(&db, "alloc-1"), 0.0, "nothing may be applied");
}

// ─── Fail-closed cross-checks and the persisted guard ───────────────────────

#[test]
fn fiscal_year_mismatch_is_rejected() {
    let mut db = ConnectionFactory::new_for_test().expect("db");
    seed(&mut db);

    let pkg = package(dataset(vec![fact("alloc-1", 10.0, 2025)]), UNIT);
    let err = apply_package(&mut db, &pkg).expect_err("fiscal cross-check must fail");
    assert!(matches!(
        err,
        AppError::Validation(ValidationError::InvalidFormat { ref field, .. }) if field == "fiscal_year"
    ));
    assert_eq!(fulfilled_of(&db, "alloc-1"), 0.0);
}

#[test]
fn unit_triple_mismatch_is_rejected() {
    let mut db = ConnectionFactory::new_for_test().expect("db");
    seed(&mut db);

    let mut wrong = fact("alloc-1", 10.0, 2026);
    wrong.conversion_factor = Some(12);
    let pkg = package(dataset(vec![wrong]), UNIT);
    let err = apply_package(&mut db, &pkg).expect_err("unit cross-check must fail");
    assert!(matches!(
        err,
        AppError::Validation(ValidationError::InvalidFormat { ref field, .. }) if field == "purchase_unit"
    ));
    assert_eq!(fulfilled_of(&db, "alloc-1"), 0.0);
}

/// ADR-0061 testing obligation 15 — the positive counterpart of
/// `unit_triple_mismatch_is_rejected`.
///
/// The unit triple is a CROSS-CHECK only, so a genuinely converting product
/// must sync unchanged in both directions. `fulfilled_quantity` is
/// **purchase-denominated** (ADR-0061 §5): at a factor of 5 an implementation
/// that converted during sync would land 200 where 40 belongs, so asserting the
/// verbatim 40 is what proves the quantity is carried, never converted.
#[test]
fn a_non_one_conversion_factor_round_trips_in_purchase_units() {
    const FACTOR: i32 = 5;
    const PURCHASE_QUANTITY: f64 = 40.0;

    // Separate source and destination databases: the export must be produced by
    // the real producer from persisted state, then applied by the real importer.
    let mut source = ConnectionFactory::new_for_test().expect("source db");
    let mut dest = ConnectionFactory::new_for_test().expect("destination db");
    seed(&mut source);
    seed(&mut dest);

    // A factor above 1 is only legal when the two units differ (schema CHECK),
    // so this also exercises a `purchase_unit` other than 1.
    for db in [&mut source, &mut dest] {
        db.executor()
            .execute(
                "UPDATE contract_products
                    SET purchase_unit = 2, consumption_unit = 1, conversion_factor = ?1
                  WHERE id = 'cp-1'",
                rusqlite::params![FACTOR],
            )
            .expect("seed a genuinely converting unit triple");
    }

    ContractRepository::new(source.executor())
        .try_increment_fulfilled("alloc-1", PURCHASE_QUANTITY)
        .expect("source-side fulfillment");

    // ── Export on the source ──
    let (payload, exported_id) = build_dataset(source.executor()).expect("export");
    assert_eq!(payload.facts.len(), 1);
    let exported = &payload.facts[0];
    assert_eq!(exported.allocation_id, "alloc-1");
    assert_eq!(exported.purchase_unit, Some(2));
    assert_eq!(
        exported.conversion_factor,
        Some(FACTOR),
        "the producer must carry the local unit triple verbatim"
    );

    // ── Import on the WILAYA side ──
    let pkg = package(payload, UNIT);
    assert_eq!(
        pkg.metadata.package_id, exported_id,
        "the importer validates the identity the producer actually issued"
    );
    let outcome = apply_package(&mut dest, &pkg).expect("a matching unit triple must import");
    assert_eq!(outcome.applied_count, 1);
    assert_eq!(outcome.already_satisfied_count, 0);

    // ── Converged to the purchase quantity, not the converted one ──
    let converged = fulfilled_of(&dest, "alloc-1");
    assert_eq!(
        converged, PURCHASE_QUANTITY,
        "fulfillment is purchase-denominated and must not be converted by sync"
    );
    assert_ne!(
        converged,
        PURCHASE_QUANTITY * FACTOR as f64,
        "a converted quantity would wrongly land at 200"
    );
}

#[test]
fn unknown_allocation_is_rejected() {
    let mut db = ConnectionFactory::new_for_test().expect("db");
    seed(&mut db);

    let pkg = package(dataset(vec![fact("alloc-missing", 10.0, 2026)]), UNIT);
    let err = apply_package(&mut db, &pkg).expect_err("unknown allocation must fail");
    assert!(matches!(
        err,
        AppError::BusinessLogic(BusinessLogicError::ResourceNotFound { .. })
    ));
}

#[test]
fn negative_quantity_is_rejected() {
    let mut db = ConnectionFactory::new_for_test().expect("db");
    seed(&mut db);

    let pkg = package(dataset(vec![fact("alloc-1", -5.0, 2026)]), UNIT);
    let err = apply_package(&mut db, &pkg).expect_err("a negative cumulative state must fail");
    assert!(matches!(
        err,
        AppError::Validation(ValidationError::InvalidFormat { ref field, .. }) if field == "fulfilled_quantity"
    ));
    assert_eq!(fulfilled_of(&db, "alloc-1"), 0.0);
}

#[test]
fn quantity_beyond_the_remaining_contract_is_rejected() {
    let mut db = ConnectionFactory::new_for_test().expect("db");
    seed(&mut db);

    let contracts = ContractRepository::new(db.executor());
    contracts
        .try_increment_released("alloc-1", 60.0)
        .expect("local release");

    // 50 + 60 released > 100 contracted — the persisted guard would reject the
    // update; the pre-check turns it into an actionable package-level rejection.
    let pkg = package(dataset(vec![fact("alloc-1", 50.0, 2026)]), UNIT);
    let err = apply_package(&mut db, &pkg).expect_err("guard violation must fail");
    assert!(matches!(
        err,
        AppError::Validation(ValidationError::InvalidFormat { ref field, .. }) if field == "fulfilled_quantity"
    ));
    assert_eq!(fulfilled_of(&db, "alloc-1"), 0.0, "state must be untouched");
}

#[test]
fn import_is_all_or_nothing() {
    let mut db = ConnectionFactory::new_for_test().expect("db");
    seed(&mut db);

    // alloc-1 is valid, alloc-2 crosses the guard (100 fulfilled + 60 released).
    ContractRepository::new(db.executor())
        .try_increment_released("alloc-2", 60.0)
        .expect("local release");
    let pkg = package(
        dataset(vec![
            fact("alloc-1", 30.0, 2026),
            fact("alloc-2", 50.0, 2026),
        ]),
        UNIT,
    );

    apply_package(&mut db, &pkg).expect_err("one invalid fact rejects the package");
    assert_eq!(
        fulfilled_of(&db, "alloc-1"),
        0.0,
        "the valid fact in a rejected package must NOT be applied"
    );
    assert_eq!(fulfilled_of(&db, "alloc-2"), 0.0);
}

#[test]
fn a_rejected_package_leaves_no_replay_marker() {
    let mut db = ConnectionFactory::new_for_test().expect("db");
    seed(&mut db);

    let bad = package(dataset(vec![fact("alloc-1", 10.0, 2025)]), UNIT);
    apply_package(&mut db, &bad).expect_err("rejected");
    // The rollback must also undo the registry row, otherwise a corrected
    // package could never be imported under a future identity either.
    let good = package(dataset(vec![fact("alloc-1", 10.0, 2026)]), UNIT);
    let outcome = apply_package(&mut db, &good).expect("corrected package applies");
    assert_eq!(outcome.applied_count, 1);
}

// ─── Entitlement state is never a filter ────────────────────────────────────

#[test]
fn fulfillment_converges_for_a_cancelled_allocation() {
    let mut db = ConnectionFactory::new_for_test().expect("db");
    seed(&mut db);

    ContractRepository::new(db.executor())
        .set_entitlement_state("alloc-1", "CANCELLED")
        .expect("cancel locally");

    let pkg = package(dataset(vec![fact("alloc-1", 15.0, 2026)]), UNIT);
    let outcome = apply_package(&mut db, &pkg).expect("a cancelled allocation still converges");
    assert_eq!(outcome.applied_count, 1);
    assert_eq!(fulfilled_of(&db, "alloc-1"), 15.0);
}

// ─── Source provenance ──────────────────────────────────────────────────────

#[test]
fn foreign_source_node_is_rejected() {
    let mut db = ConnectionFactory::new_for_test().expect("db");
    seed(&mut db);

    let pkg = package(
        dataset(vec![fact("alloc-1", 10.0, 2026)]),
        "some-other-node",
    );
    let err = apply_package(&mut db, &pkg).expect_err("foreign source must fail");
    assert!(matches!(
        err,
        AppError::Validation(ValidationError::InvalidFormat { ref field, .. }) if field == "source_node_id"
    ));
    assert_eq!(fulfilled_of(&db, "alloc-1"), 0.0);
}

#[test]
fn another_units_allocation_is_rejected_even_inside_the_same_wilaya() {
    let mut db = ConnectionFactory::new_for_test().expect("db");
    seed(&mut db);

    // A second UNIT of the SAME wilaya, holding its own allocation. The fact
    // carries no unit_id by design, so without an ownership check any
    // authenticated UNIT of this wilaya could drive a peer's state.
    UnitRepository::new(db.executor())
        .upsert_raw_unit("unit-b", "B1", "unit b", WILAYA, STAMP)
        .expect("seed peer unit");
    ContractRepository::new(db.executor())
        .insert_allocation(
            "alloc-peer",
            "ctr-1",
            "cp-2",
            "unit-b",
            "prod-2",
            2026,
            100.0,
            STAMP,
        )
        .expect("seed peer allocation");

    // Source is `UNIT`, but the fact names `unit-b`'s allocation.
    let pkg = package(dataset(vec![fact("alloc-peer", 40.0, 2026)]), UNIT);
    let err = apply_package(&mut db, &pkg).expect_err("a peer's allocation must fail");
    assert!(matches!(
        err,
        AppError::Validation(ValidationError::InvalidFormat { ref field, .. }) if field == "allocation_id"
    ));
    assert_eq!(
        fulfilled_of(&db, "alloc-peer"),
        0.0,
        "the peer's state must be untouched"
    );
}

// ─── Projection ──────────────────────────────────────────────────────────────

#[test]
fn wilaya_projection_exposes_both_remainders() {
    let mut db = ConnectionFactory::new_for_test().expect("db");
    seed(&mut db);

    let pkg = package(dataset(vec![fact("alloc-1", 30.0, 2026)]), UNIT);
    apply_package(&mut db, &pkg).expect("apply");

    let allocation = ContractRepository::new(db.executor())
        .get_allocation("alloc-1")
        .expect("read")
        .expect("exists");
    // 100 contracted − 30 fulfilled = 70 for both, because nothing is released
    // or reserved here. `effective_remaining` and the ADR-0061 remainder are
    // separate projections and must agree here.
    assert_eq!(allocation.effective_remaining(), 70.0);
    assert_eq!(allocation.wilaya_executable_remaining(), 70.0);
}
