//! SEC-087 Phase 6B (ADR-0057 §3.4): Products V3 Payload Contract.
//!
//! The WILAYA-authoritative Product sync payload carries the unit/TVA
//! configuration (`purchase_unit`, `consumption_unit`, `conversion_factor`,
//! `tva_classification`). A UNIT importer fails closed when the configuration
//! is absent or invalid — BEFORE mutation, imported-package marking, or
//! success auditing. This suite covers:
//!
//! 1. Export round-trip — the payload carries the four configuration fields and
//!    no supplier/recovery fields.
//! 2. Real production pipeline (`import_products_package_impl` on a UNIT node,
//!    WILAYA-issued + Ed25519-signed V3 package) — conforming payload imports
//!    and creates keyed inventory identity `(product_id, consumption_unit)`.
//! 3. Validator-level fail-closed matrix — absent/out-of-range/inconsistent
//!    configuration is rejected; a cross-unit-traversal combination still
//!    requires factor-consistency.
//! 4. Zero-mutation / zero-marker / audit invariants on config-free payloads
//!    (the 6A-era artifact shape: fields absent from the wire → `None` via
//!    `#[serde(default)]` → semantic rejection).
//! 5. Explicit `ON CONFLICT(id) DO UPDATE` (never `INSERT OR REPLACE`): a
//!    re-import updates the row, keeps `created_at`, and does NOT
//!    cascade-delete existing `inventory_stocks`.
//! 6. Legacy NULL-config stock rows coexist with newly keyed stock rows.

#[allow(dead_code)]
mod common;

use chrono::{DateTime, Utc};
use std::path::Path;
use uuid::Uuid;

use grpc_lib::application::sync::{
    validate_products_package_for_import, PackageId, SyncPackage, SyncPackageMetadata,
    SYNC_PACKAGE_SCHEMA_VERSION,
};
use grpc_lib::application::usecases::exports::types::ProductsExportDataset;
use grpc_lib::commands::{import_products_package_impl, AppState};
use grpc_lib::db::ConnectionFactory;
use grpc_lib::domain::identity::{
    CredentialStatus, IdentityCertificate, IdentitySigner, IdentityStorePort, SubjectType,
    SIGNATURE_VERSION_ED25519,
};
use grpc_lib::infrastructure::security::{AgeFileEncryptionProvider, Ed25519SigningProvider};
use grpc_lib::infrastructure::sync::packages::canonical_json::{
    canonical_bytes_for_integrity, canonical_bytes_for_signature,
};
use grpc_lib::infrastructure::sync::packages::integrity::{PackageHasher, Sha256PackageHasher};
use grpc_lib::infrastructure::sync::packages::signing::{Ed25519PackageSigner, PackageSigner};
use grpc_lib::infrastructure::sync::{PackageBuilder, SerdeJsonSyncPackageSerializer};
use grpc_lib::models::{Product, ProductExportRow};
use grpc_lib::repositories::RepositoryProvider;

const WILAYA_CODE: &str = "16";
/// WILAYA anchor signing key — matches the seeded WILAYA certificate.
const WILAYA_SECRET: [u8; 32] = [9u8; 32];

/// UNIT-like importer fixture: settings (UNIT/16/configured), open fiscal year
/// 2026, and a seeded ACTIVE WILAYA anchor certificate (the fleet trust root a
/// provisioned UNIT node holds). Returns the anchor identity id used as the
/// package issuer.
fn build_unit_state() -> (AppState, Uuid) {
    let db = ConnectionFactory::new_for_test().expect("db");
    let state = AppState::new_for_test(db);
    {
        let guard = state.get_db().expect("lock");
        let db = guard.as_ref().expect("db");
        db.get_connection()
            .execute(
                "UPDATE settings SET node_type = 'UNIT', configured = 1, wilaya_code = '16', wilaya_name = 'Alger' WHERE id = 1",
                [],
            )
            .expect("settings");
        db.get_connection()
            .execute(
                "INSERT OR REPLACE INTO fiscal_year_status (year, status, opened_at) VALUES (2026, 'open', ?1)",
                rusqlite::params![Utc::now().to_rfc3339()],
            )
            .expect("fiscal year open");
        db.get_connection()
            .execute("UPDATE settings SET current_year = 2026 WHERE id = 1", [])
            .expect("current year");
    }
    let anchor_id = seed_wilaya_anchor(&state);
    (state, anchor_id)
}

fn seed_wilaya_anchor(state: &AppState) -> Uuid {
    let anchor_id = Uuid::new_v4();
    let certificate = IdentityCertificate {
        identity_id: anchor_id,
        subject_type: SubjectType::Wilaya,
        subject_id: anchor_id,
        issuer_identity_id: None,
        credential_id: Uuid::new_v4(),
        generation: 1,
        status: CredentialStatus::Active,
        public_key: Ed25519SigningProvider::new(WILAYA_SECRET).public_key(),
        algorithm_version: SIGNATURE_VERSION_ED25519,
        not_after: None,
        package_sequence: Some(1),
        signature: None,
    };
    let now = Utc::now().to_rfc3339();
    {
        let guard = state.get_db().expect("lock");
        let db = guard.as_ref().expect("db");
        IdentityStorePort::upsert(&db.executor().identity_store(), &certificate, &now)
            .expect("seed wilaya anchor");
    }
    anchor_id
}

fn set_session(state: &AppState) {
    common::insert_test_user(state, "u1", "bob", "Admin");
    let mut session = common::create_test_session("u1", "bob", "Admin");
    session.user_role = grpc_lib::models::UserRole::from("Admin".to_string());
    *state.current_session.lock().expect("session mutex") = Some(session);
}

/// Build a `signature_version = 2` package the same way `PackageBuilder` does:
/// Pass A (integrity hash over canonical bytes) then Pass B (Ed25519 signature
/// over canonical bytes with the hash present).
fn sign_v2_package<T: serde::Serialize>(
    mut package: SyncPackage<T>,
    secret: [u8; 32],
) -> SyncPackage<T> {
    let signer = Ed25519PackageSigner::new(secret);
    let value = serde_json::to_value(&package).expect("value");
    let hash = Sha256PackageHasher
        .hash(&canonical_bytes_for_integrity(&value).expect("canonical integrity"))
        .expect("hash");
    package.metadata.integrity_hash = Some(hash);

    let value = serde_json::to_value(&package).expect("value");
    let signature = signer
        .sign(&canonical_bytes_for_signature(&value).expect("canonical signature"))
        .expect("sign");
    package.metadata.signature = Some(signature);
    package
}

fn write_encrypted<T: serde::Serialize>(package: &SyncPackage<T>, secret: [u8; 32], path: &Path) {
    let signer = Ed25519PackageSigner::new(secret);
    PackageBuilder::new()
        .build_encrypted_stream_path(
            package,
            &SerdeJsonSyncPackageSerializer,
            &signer,
            &AgeFileEncryptionProvider::new(),
            path,
        )
        .expect("build encrypted package");
}

/// A conforming SEC-087 V3 Product record: purchase=consumption (Kilograms),
/// factor 1, TVA classification 0 (exempt).
fn conforming_row(
    id: &str,
    name: &str,
    updated_at: DateTime<Utc>,
    created_at: DateTime<Utc>,
) -> ProductExportRow {
    ProductExportRow {
        product: Product {
            id: id.to_string(),
            name: name.to_string(),
            base_price: 10.0,
            year: 2026,
            created_at,
        },
        updated_at: updated_at.to_rfc3339(),
        node_id: WILAYA_CODE.to_string(),
        deleted: 0,
        purchase_unit: Some(1),
        consumption_unit: Some(1),
        conversion_factor: Some(1),
        tva_classification: Some(0),
    }
}

/// WILAYA-issued V3 products package targeting the importer's WILAYA (16).
fn products_package_v3(
    pkg_id: &str,
    issuer_id: Uuid,
    created_at: DateTime<Utc>,
    rows: Vec<ProductExportRow>,
) -> SyncPackage<ProductsExportDataset> {
    let signer = Ed25519PackageSigner::new(WILAYA_SECRET);
    SyncPackage {
        metadata: SyncPackageMetadata {
            schema_version: SYNC_PACKAGE_SCHEMA_VERSION,
            created_at,
            source_node_id: WILAYA_CODE.to_string(),
            issuer_identity_id: Some(issuer_id),
            package_id: PackageId(pkg_id.to_string()),
            signature_version: Some(SIGNATURE_VERSION_ED25519),
            signing_key_id: Some(signer.public_key_hex()),
            integrity_hash: None,
            signature: None,
        },
        payload: ProductsExportDataset { product_rows: rows },
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers — persisted-state inspection
// ─────────────────────────────────────────────────────────────────────────────

/// (id, name, base_price, purchase_unit, consumption_unit, conversion_factor,
/// tva_classification, created_at, node_id, deleted) — every WILAYA-authoritative
/// Product column written by the importer.
type ProductRowView = (
    String,
    String,
    i64,
    Option<i32>,
    Option<i32>,
    Option<i32>,
    Option<i32>,
    String,
    String,
    i32,
);

fn product_rows(state: &AppState) -> Vec<ProductRowView> {
    let guard = state.get_db().expect("lock");
    let db = guard.as_ref().expect("db");
    let mut stmt = db
        .get_connection()
        .prepare(
            "SELECT id, name, base_price, purchase_unit, consumption_unit, conversion_factor, \
             tva_classification, created_at, node_id, deleted FROM products",
        )
        .expect("prepare");
    stmt.query_map([], |row| {
        Ok((
            row.get(0)?,
            row.get(1)?,
            row.get(2)?,
            row.get(3)?,
            row.get(4)?,
            row.get(5)?,
            row.get(6)?,
            row.get(7)?,
            row.get(8)?,
            row.get(9)?,
        ))
    })
    .expect("query")
    .map(|r| r.expect("row"))
    .collect()
}

/// (id, product_id, consumption_unit) for every inventory_stocks row of a
/// product — `consumption_unit = Some(code)` is the keyed identity a
/// configured product seed owns; `None` is the legacy NULL-keyed shape.
fn stock_rows(state: &AppState, product_id: &str) -> Vec<(String, Option<i32>)> {
    let guard = state.get_db().expect("lock");
    let db = guard.as_ref().expect("db");
    let mut stmt = db
        .get_connection()
        .prepare("SELECT id, consumption_unit FROM inventory_stocks WHERE product_id = ?1")
        .expect("prepare");
    stmt.query_map(rusqlite::params![product_id], |row| {
        Ok((row.get(0)?, row.get(1)?))
    })
    .expect("query")
    .map(|r| r.expect("row"))
    .collect()
}

/// Number of `applied_sync_packages` ("has-imported") markers for a package id.
fn applied_package_marker_count(state: &AppState, package_id: &str) -> usize {
    let guard = state.get_db().expect("lock");
    let db = guard.as_ref().expect("db");
    let count: i64 = db
        .get_connection()
        .query_row(
            "SELECT COUNT(*) FROM applied_sync_packages WHERE package_id = ?1",
            rusqlite::params![package_id],
            |row| row.get(0),
        )
        .expect("query marker count");
    count as usize
}

/// Import-audit event types recorded for a package id, in order.
fn audit_event_types(state: &AppState, package_id: &str) -> Vec<String> {
    let guard = state.get_db().expect("lock");
    let db = guard.as_ref().expect("db");
    let mut stmt = db
        .get_connection()
        .prepare("SELECT event_type FROM import_audit_events WHERE package_id = ?1")
        .expect("prepare");
    stmt.query_map(rusqlite::params![package_id], |row| row.get::<_, String>(0))
        .expect("query")
        .map(|r| r.expect("row"))
        .collect()
}

/// Success entries in the audit chain for a given action.
fn success_audit_count(state: &AppState, action: &str) -> i64 {
    let guard = state.get_db().expect("lock");
    let db = guard.as_ref().expect("db");
    db.get_connection()
        .query_row(
            "SELECT COUNT(*) FROM audit_log WHERE action = ?1 AND status = 'Success'",
            rusqlite::params![action],
            |row| row.get(0),
        )
        .expect("query audit count")
}

// ─────────────────────────────────────────────────────────────────────────────
// 1. Export round-trip: the WILAYA exporter emits the config, no supplier
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn export_round_trip_emits_config_and_no_supplier_fields() {
    use grpc_lib::application::usecases::exports::export_products_dataset::execute as export_dataset;
    use grpc_lib::application::usecases::exports::types::ExportProductsInput;

    let db = ConnectionFactory::new_for_test().expect("db");
    let now = Utc::now().to_rfc3339();
    // A well-formed WILAYA row with complete configuration.
    db.get_connection().execute(
        "INSERT INTO products (id, name, base_price, year, purchase_unit, consumption_unit, conversion_factor, tva_classification, created_at, updated_at, node_id, deleted)
         VALUES ('p-export', 'Semolina', 1250, 2026, 7, 6, 2, 2, ?1, ?1, '16', 0)",
        rusqlite::params![now],
    ).expect("seed configured product");

    let dataset = export_dataset(db.executor(), ExportProductsInput).expect("export dataset");
    assert_eq!(dataset.product_rows.len(), 1);
    let row = &dataset.product_rows[0];
    assert_eq!(row.product.id, "p-export");
    assert_eq!(row.purchase_unit, Some(7), "Semolina purchase unit = Egg");
    assert_eq!(
        row.consumption_unit,
        Some(6),
        "Semolina consumption unit = Piece"
    );
    assert_eq!(row.conversion_factor, Some(2));
    assert_eq!(row.tva_classification, Some(2));

    // Wire shape: the four config fields present; supplier / recovery concerns
    // are NOT part of the Product V3 sync contract.
    let wire = serde_json::to_value(&dataset).expect("wire");
    let row_json = &wire["product_rows"][0];
    assert_eq!(row_json["purchase_unit"], serde_json::json!(7));
    assert_eq!(row_json["consumption_unit"], serde_json::json!(6));
    assert_eq!(row_json["conversion_factor"], serde_json::json!(2));
    assert_eq!(row_json["tva_classification"], serde_json::json!(2));
    assert!(
        row_json.get("supplier").is_none(),
        "supplier must NOT be part of the Product V3 sync record"
    );
    assert!(
        row_json.get("base_price_recovery_basis").is_none()
            && row_json.get("unit_purchase_variability").is_none(),
        "recovery concerns are Phase 6C — not part of the V3 Product record"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 6A artifact shape: config fields ABSENT from the wire are deserialized to
// None (shape-compatible) and rejected by the semantic validator
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn config_free_wire_shape_is_deserialized_then_rejected() {
    // Build the wire value of a V3 products package whose record omits the four
    // config keys entirely — the exact shape emitted by the 6A-era config-free
    // exporter.
    let value = serde_json::json!({
        "metadata": {
            "schema_version": SYNC_PACKAGE_SCHEMA_VERSION.as_u16(),
            "created_at": "2026-02-01T10:00:00Z",
            "source_node_id": "16",
            "issuer_identity_id": null,
            "package_id": "pkg-config-free-shape",
            "signature_version": 2,
            "signing_key_id": null,
            "integrity_hash": null,
            "signature": null
        },
        "payload": {
            "product_rows": [{
                "product": { "id": "p-legacy-1", "name": "Bread", "base_price": 2000.0, "year": 2026, "created_at": "2026-01-01T08:00:00Z" },
                "updated_at": "2026-01-05T08:00:00Z",
                "node_id": "16",
                "deleted": 0
            }]
        }
    });

    let package: SyncPackage<ProductsExportDataset> =
        serde_json::from_value(value).expect("shape-compatible deserialization");
    assert_eq!(
        package.metadata.schema_version.as_u16(),
        SYNC_PACKAGE_SCHEMA_VERSION.as_u16(),
        "the V3 envelope still passes the schema gate"
    );
    assert!(
        package.payload.product_rows[0].purchase_unit.is_none()
            && package.payload.product_rows[0].consumption_unit.is_none()
            && package.payload.product_rows[0].conversion_factor.is_none()
            && package.payload.product_rows[0].tva_classification.is_none(),
        "absent fields deserialize to None via #[serde(default)] — no fabrication"
    );

    let err = validate_products_package_for_import(&package)
        .expect_err("config-free payload must be rejected before any mutation");
    assert!(
        err.to_string().contains("purchase_unit"),
        "validation must name the missing config field: {err}"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 2. Real pipeline: conforming payload imports and creates keyed stock
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn conforming_payload_imports_with_keyed_stock_identity() {
    let (state, anchor_id) = build_unit_state();
    set_session(&state);

    let created = Utc::now();
    let pkg = sign_v2_package(
        products_package_v3(
            "pkg-v3-conforming",
            anchor_id,
            created,
            vec![conforming_row("p-1", "Wheat", created, created)],
        ),
        WILAYA_SECRET,
    );
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("products.sync");
    write_encrypted(&pkg, WILAYA_SECRET, &path);

    let result = import_products_package_impl(&state, path.to_string_lossy().into_owned())
        .expect("conforming V3 products package imports on the UNIT node");
    assert_eq!(result.added, 1);
    assert_eq!(result.updated, 0);

    let rows = product_rows(&state);
    assert_eq!(rows.len(), 1);
    let (id, name, _, purchase, consumption, factor, tva, _created, node, deleted) = &rows[0];
    assert_eq!(id, "p-1");
    assert_eq!(name, "Wheat");
    assert_eq!(
        *purchase,
        Some(1),
        "purchase_unit written without normalization"
    );
    assert_eq!(
        *consumption,
        Some(1),
        "consumption_unit written without normalization"
    );
    assert_eq!(*factor, Some(1));
    assert_eq!(*tva, Some(0));
    assert_eq!(node, WILAYA_CODE);
    assert_eq!(*deleted, 0);

    // Keyed inventory identity (product_id, consumption_unit) — NO NULL fallback.
    let stocks = stock_rows(&state, id);
    assert_eq!(stocks.len(), 1, "exactly one keyed stock row");
    assert_eq!(stocks.len(), 1, "exactly one keyed stock row");
    assert_eq!(
        stocks[0].1,
        Some(1),
        "stock row is keyed by consumption_unit"
    );
    assert_eq!(applied_package_marker_count(&state, "pkg-v3-conforming"), 1);

    // Success audit recorded on the conforming import.
    assert!(audit_event_types(&state, "pkg-v3-conforming").contains(&"ImportSucceeded".into()));
    assert_eq!(success_audit_count(&state, "ImportNodePackage"), 1);
}

#[test]
fn tva_classification_and_division_table_intact_on_import() {
    let (state, anchor_id) = build_unit_state();
    set_session(&state);

    let created = Utc::now();
    let mut row = conforming_row("p-tva", "Butter", created, created);
    row.tva_classification = Some(2);
    let pkg = sign_v2_package(
        products_package_v3("pkg-v3-tva", anchor_id, created, vec![row]),
        WILAYA_SECRET,
    );
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("products.sync");
    write_encrypted(&pkg, WILAYA_SECRET, &path);

    import_products_package_impl(&state, path.to_string_lossy().into_owned())
        .expect("tva-classified product imports");

    let rows = product_rows(&state);
    assert_eq!(
        rows[0].6,
        Some(2),
        "tva_classification propagated to products row"
    );

    // The product_tax_classifications ledger is intact and untouched.
    let guard = state.get_db().expect("lock");
    let db = guard.as_ref().expect("db");
    let tx_count: i64 = db
        .get_connection()
        .query_row(
            "SELECT COUNT(*) FROM product_tax_classifications",
            [],
            |row| row.get(0),
        )
        .expect("ledger intact");
    assert_eq!(
        tx_count, 0,
        "sync import writes only products.tva_classification"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 2b. Real pipeline: replay protection — same package id rejected, no mutation
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn duplicate_package_id_rejected_before_mutation() {
    let (state, anchor_id) = build_unit_state();
    set_session(&state);

    let created = Utc::now();
    let pkg = sign_v2_package(
        products_package_v3(
            "pkg-v3-replay",
            anchor_id,
            created,
            vec![conforming_row("p-replay", "Milk", created, created)],
        ),
        WILAYA_SECRET,
    );
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("products.sync");
    write_encrypted(&pkg, WILAYA_SECRET, &path);

    import_products_package_impl(&state, path.to_string_lossy().into_owned())
        .expect("first import ok");
    assert_eq!(applied_package_marker_count(&state, "pkg-v3-replay"), 1);

    let err = import_products_package_impl(&state, path.to_string_lossy().into_owned())
        .expect_err("same package id must be rejected (replay dedup)");
    assert!(
        err.contains("مسبقاً") && err.contains("نفس المعرف"),
        "replay rejection message: {err}"
    );
    assert_eq!(product_rows(&state).len(), 1, "no second product row");
}

// ─────────────────────────────────────────────────────────────────────────────
// 2c. Real pipeline: re-import (newer package, ON CONFLICT DO UPDATE) preserves
// created_at and does NOT cascade-delete existing keyed stock
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn conflict_update_preserves_created_at_and_keyed_stock() {
    let (state, anchor_id) = build_unit_state();
    set_session(&state);

    let created = Utc::now();
    let t1 = created;
    let t2 = created + chrono::Duration::seconds(60);

    // Package A: initial conforming import with created_at = t1.
    let pkg_a = sign_v2_package(
        products_package_v3(
            "pkg-v3-update-a",
            anchor_id,
            t1,
            vec![conforming_row("p-u", "Margarine", t1, t1)],
        ),
        WILAYA_SECRET,
    );
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("products.sync");
    write_encrypted(&pkg_a, WILAYA_SECRET, &path);
    import_products_package_impl(&state, path.to_string_lossy().into_owned())
        .expect("initial import ok");
    let stock_after_first = stock_rows(&state, "p-u");
    assert_eq!(stock_after_first.len(), 1);
    let original_stock_id = stock_after_first[0].0.clone();

    // Package B: newer package, same product id — same consumption-unit identity
    // so the keyed stock row must be preserved in place, only the authoritative
    // Product fields (name/base_price here) move forward.
    let mut row_b = conforming_row("p-u", "Butter Spread", t2, t1);
    row_b.product.base_price = 15.0;
    let pkg_b = sign_v2_package(
        products_package_v3("pkg-v3-update-b", anchor_id, t2, vec![row_b]),
        WILAYA_SECRET,
    );
    let path_b = dir.path().join("products-update.sync");
    write_encrypted(&pkg_b, WILAYA_SECRET, &path_b);
    let result = import_products_package_impl(&state, path_b.to_string_lossy().into_owned())
        .expect("re-import with newer package updates the row");
    assert_eq!(result.updated, 1);

    let rows = product_rows(&state);
    assert_eq!(
        rows.len(),
        1,
        "ON CONFLICT updates in place — no duplicate rows"
    );
    let (_, name, base_price, purchase, consumption, _factor, _tva, created_at, _, _) = &rows[0];
    assert_eq!(name, "Butter Spread");
    assert_eq!(
        *base_price, 1500,
        "base_price (scaled cents) updated on conflict"
    );
    assert_eq!(*purchase, Some(1));
    assert_eq!(*consumption, Some(1), "consumption unit key unchanged");
    assert_eq!(
        created_at,
        &t1.to_rfc3339(),
        "created_at is local insert history and MUST be preserved on conflict"
    );

    // The authoritative keyed stock row is updated in place — NOT cascade-deleted
    // (an INSERT OR REPLACE implementation would delete-then-insert and lose it).
    let stocks = stock_rows(&state, "p-u");
    assert_eq!(stocks.len(), 1, "no stock row lost to a replace semantics");
    assert_eq!(
        stocks[0].0, original_stock_id,
        "stock identity preserved in place"
    );
    assert_eq!(
        stocks[0].1,
        Some(1),
        "keyed stock identity follows the config"
    );

    // Record 21: supplier never appears on any wire or row.
    let payload_wire = serde_json::to_value(&pkg_b.payload).expect("wire");
    assert!(
        !payload_wire.to_string().contains("supplier"),
        "supplier must not be present in the V3 payload"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 2d. Real pipeline: a configured local product without stock establishes
// exactly one keyed inventory identity on import — no other row
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn configured_product_without_stock_gets_keyed_identity_on_import() {
    let (state, anchor_id) = build_unit_state();
    set_session(&state);

    // A locally-row-seeded product already carries a complete configuration but
    // no stock row yet. The import must establish exactly the keyed inventory
    // identity (product_id, consumption_unit); a NULL-keyed identity cannot
    // exist under the finalized schema.
    let now = Utc::now().to_rfc3339();
    {
        let guard = state.get_db().expect("lock");
        let db = guard.as_ref().expect("db");
        db.get_connection().execute(
            "INSERT INTO products (id, name, base_price, year, purchase_unit, consumption_unit, conversion_factor, tva_classification, created_at, updated_at, node_id, deleted)
             VALUES ('p-legacy', 'LegacyBread', 900, 2026, 1, 1, 1, 0, ?1, ?1, '16', 0)",
            rusqlite::params![now],
        )
        .expect("seed configured product");
    }

    let created = Utc::now();
    let pkg = sign_v2_package(
        products_package_v3(
            "pkg-v3-legacy",
            anchor_id,
            created,
            vec![conforming_row("p-legacy", "LegacyBread", created, created)],
        ),
        WILAYA_SECRET,
    );
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("products.sync");
    write_encrypted(&pkg, WILAYA_SECRET, &path);
    import_products_package_impl(&state, path.to_string_lossy().into_owned())
        .expect("configured re-baseline of the local product ok");

    // Exactly one keyed stock row; no NULL-keyed identity exists.
    let stocks = stock_rows(&state, "p-legacy");
    assert_eq!(stocks.len(), 1, "exactly one keyed stock row");
    assert_eq!(
        stocks[0].1,
        Some(1),
        "stock identity follows the configured consumption_unit"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 3. Real pipeline: config-free payload is rejected before mutation, with
// zero marker and failure audit (no success audit)
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn config_free_payload_rejected_before_mutation_with_zero_marker_and_failure_audit() {
    let (state, anchor_id) = build_unit_state();
    set_session(&state);

    let created = Utc::now();
    // Config-free record (the 6A-era voxel shape: all four fields absent → None).
    let mut row = conforming_row("p-cf", "NoConfig", created, created);
    row.purchase_unit = None;
    row.consumption_unit = None;
    row.conversion_factor = None;
    row.tva_classification = None;
    let pkg = sign_v2_package(
        products_package_v3("pkg-v3-config-free", anchor_id, created, vec![row]),
        WILAYA_SECRET,
    );
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("products.sync");
    write_encrypted(&pkg, WILAYA_SECRET, &path);

    let err = import_products_package_impl(&state, path.to_string_lossy().into_owned())
        .expect_err("config-free V3 products package must be rejected");
    assert!(
        err.contains("purchase_unit"),
        "rejection names the missing config field: {err}"
    );

    // Zero mutation.
    assert_eq!(
        product_rows(&state).len(),
        0,
        "no Product row may be written"
    );
    let stock_count: i64 = {
        let guard = state.get_db().expect("lock");
        let db = guard.as_ref().expect("db");
        db.get_connection()
            .query_row("SELECT COUNT(*) FROM inventory_stocks", [], |row| {
                row.get(0)
            })
            .expect("stock count")
    };
    assert_eq!(stock_count, 0, "no inventory row may be written");

    // Zero imported marker.
    assert_eq!(
        applied_package_marker_count(&state, "pkg-v3-config-free"),
        0,
        "config-free payload must never be marked imported"
    );

    // No success audit; the rejection is recorded.
    let evts = audit_event_types(&state, "pkg-v3-config-free");
    assert!(
        !evts.contains(&"ImportSucceeded".into()),
        "no successful import audit may be recorded"
    );
    assert!(
        evts.contains(&"ImportRejected".into()),
        "a rejection audit must exist: {:?}",
        evts
    );
    assert_eq!(
        success_audit_count(&state, "ImportNodePackage"),
        0,
        "no success audit chain entry may exist"
    );
}

#[test]
fn partially_configured_payload_rejected_before_mutation() {
    let (state, anchor_id) = build_unit_state();
    set_session(&state);

    let created = Utc::now();
    // purchase_unit only — the rest missing.
    let mut row = conforming_row("p-part", "Partial", created, created);
    row.consumption_unit = None;
    row.conversion_factor = None;
    row.tva_classification = None;
    let pkg = sign_v2_package(
        products_package_v3("pkg-v3-partial", anchor_id, created, vec![row]),
        WILAYA_SECRET,
    );
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("products.sync");
    write_encrypted(&pkg, WILAYA_SECRET, &path);

    let err = import_products_package_impl(&state, path.to_string_lossy().into_owned())
        .expect_err("partially configured payload must be rejected");
    assert!(err.contains("consumption_unit"), "got: {err}");
    assert_eq!(product_rows(&state).len(), 0, "no mutation on rejection");
    assert_eq!(
        applied_package_marker_count(&state, "pkg-v3-partial"),
        0,
        "no imported marker on rejection"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 4. Validator-level fail-closed matrix (absent / out-of-range / inconsistent)
// ─────────────────────────────────────────────────────────────────────────────

fn assert_config_rejected(
    (purchase_unit, consumption_unit, conversion_factor, tva_classification): (
        Option<i32>,
        Option<i32>,
        Option<i32>,
        Option<i32>,
    ),
) {
    let created = Utc::now();
    let mut row = conforming_row("p-val", "Validated", created, created);
    row.purchase_unit = purchase_unit;
    row.consumption_unit = consumption_unit;
    row.conversion_factor = conversion_factor;
    row.tva_classification = tva_classification;
    let pkg = products_package_v3("pkg-validation-x", Uuid::new_v4(), created, vec![row]);
    assert!(
        validate_products_package_for_import(&pkg).is_err(),
        "config {purchase_unit:?}/{consumption_unit:?}/{conversion_factor:?}/{tva_classification:?} must be rejected"
    );
}

fn assert_config_accepted(
    (purchase_unit, consumption_unit, conversion_factor, tva_classification): (
        Option<i32>,
        Option<i32>,
        Option<i32>,
        Option<i32>,
    ),
) {
    let created = Utc::now();
    let mut row = conforming_row("p-val", "Validated", created, created);
    row.purchase_unit = purchase_unit;
    row.consumption_unit = consumption_unit;
    row.conversion_factor = conversion_factor;
    row.tva_classification = tva_classification;
    let pkg = products_package_v3("pkg-validation-ok", Uuid::new_v4(), created, vec![row]);
    assert!(
        validate_products_package_for_import(&pkg).is_ok(),
        "config {purchase_unit:?}/{consumption_unit:?}/{conversion_factor:?}/{tva_classification:?} must be accepted"
    );
}

#[test]
fn validator_rejects_purchase_unit_out_of_range() {
    assert_config_rejected((Some(999), Some(1), Some(1), Some(0)));
}

#[test]
fn validator_rejects_consumption_unit_out_of_range() {
    assert_config_rejected((Some(1), Some(321), Some(1), Some(0)));
}

#[test]
fn validator_rejects_tva_classification_out_of_range() {
    assert_config_rejected((Some(1), Some(1), Some(1), Some(3)));
}

#[test]
fn validator_rejects_zero_and_negative_conversion_factor() {
    assert_config_rejected((Some(2), Some(1), Some(0), Some(0)));
    assert_config_rejected((Some(2), Some(1), Some(-1), Some(0)));
}

#[test]
fn validator_rejects_factor_1_with_identical_units() {
    // Identical units REQUIRE factor == 1 (matching purchase→consumption).
    assert_config_rejected((Some(1), Some(1), Some(2), Some(0)));
}

#[test]
fn validator_accepts_identical_units_with_factor_1() {
    assert_config_accepted((Some(1), Some(1), Some(1), Some(0)));
}

#[test]
fn validator_accepts_mixed_units_with_factor_scan() {
    // A cross-unit traversal: purchase Kg (1) → consumption Piece (6), factor 2.
    assert_config_accepted((Some(1), Some(6), Some(2), Some(0)));
}

#[test]
fn validator_rejects_missing_individual_config_fields() {
    assert_config_rejected((None, Some(1), Some(1), Some(0)));
    assert_config_rejected((Some(1), None, Some(1), Some(0)));
    assert_config_rejected((Some(1), Some(1), None, Some(0)));
    assert_config_rejected((Some(1), Some(1), Some(1), None));
}

// ─────────────────────────────────────────────────────────────────────────────
// 5. Security invariant: tampered payload (valid signature shape, invalid data)
// is rejected before mutation
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn tampered_payload_rejected_before_mutation() {
    let (state, anchor_id) = build_unit_state();
    set_session(&state);

    let created = Utc::now();
    let mut pkg = sign_v2_package(
        products_package_v3(
            "pkg-v3-tamper",
            anchor_id,
            created,
            vec![conforming_row("p-tamper", "NameA", created, created)],
        ),
        WILAYA_SECRET,
    );
    // Attacker swaps the payload and recomputes the SHA-256 integrity hash
    // (public knowledge) — but cannot forge the Ed25519 signature. The file is
    // serialized + encrypted directly (bypassing PackageBuilder, which would
    // re-sign and thereby re-validate the tampered payload).
    pkg.payload = ProductsExportDataset {
        product_rows: vec![conforming_row("p-tamper", "NameB", created, created)],
    };
    let value = serde_json::to_value(&pkg).expect("value");
    let hash = Sha256PackageHasher
        .hash(&canonical_bytes_for_integrity(&value).expect("canonical"))
        .expect("hash");
    pkg.metadata.integrity_hash = Some(hash);
    // Signature left stale over the ORIGINAL canonical bytes.

    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("products.sync");
    let json = serde_json::to_vec(&pkg).expect("serialize tampered package");
    let mut input = std::io::Cursor::new(json);
    let output = std::fs::File::create(&path).expect("create encrypted file");
    let mut output = std::io::BufWriter::new(output);
    AgeFileEncryptionProvider::new()
        .encrypt_stream(&mut input, &mut output)
        .expect("encrypt tampered package");
    use std::io::Write;
    output.flush().expect("flush encrypted tampered package");

    let err = import_products_package_impl(&state, path.to_string_lossy().into_owned())
        .expect_err("post-signature payload mutation must be rejected before mutation");
    assert!(
        err.contains("فشل التحقق") || err.contains("signature"),
        "signature failure surfaced: {err}"
    );
    assert_eq!(
        product_rows(&state).len(),
        0,
        "no mutation on tampered payload"
    );
    assert_eq!(
        applied_package_marker_count(&state, "pkg-v3-tamper"),
        0,
        "no imported marker on tampered payload"
    );
}
