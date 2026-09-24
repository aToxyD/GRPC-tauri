//! ADR-0058 behavioral tests: Product unit-config immutability after first
//! stock movement.
//!
//! The `(purchase_unit, consumption_unit, conversion_factor)` tuple is frozen
//! once `EXISTS(SELECT 1 FROM stock_movements WHERE product_id = ?)`. A V3
//! sync package that changes a frozen tuple rejects the ENTIRE package,
//! fail-closed and before any Product/inventory mutation, inside the Product
//! V3 validation/import boundary.
//!
//! Covers the ADR-0058 §10 testing contract:
//!   1. No movement + unchanged tuple → accepted.
//!   2. No movement + changed tuple → accepted.
//!   3. Movement exists + unchanged tuple → accepted.
//!   4. Movement exists + changed `purchase_unit` → whole package rejected.
//!   5. Movement exists + changed `consumption_unit` → whole package rejected.
//!   6. Movement exists + changed `conversion_factor` → whole package rejected.
//!   7. Movement exists + changed `name`/`base_price` only → accepted.
//!   8. `tva_classification` NOT frozen by this ADR.
//!   9. Forbidden tuple change → zero Product mutation.
//!  10. Forbidden tuple change → zero inventory mutation.
//!  11. Multi-product package with one violating Product → whole package
//!      rejected, no Product changes applied.
//!  12. Corrected / re-exported conforming package → subsequently accepted.

#[allow(dead_code)]
mod common;

use chrono::{DateTime, Utc};
use std::path::Path;
use uuid::Uuid;

use grpc_lib::application::sync::{
    PackageId, SyncPackage, SyncPackageMetadata, SYNC_PACKAGE_SCHEMA_VERSION,
};
use grpc_lib::application::usecases::exports::types::ProductsExportDataset;
use grpc_lib::commands::{import_products_package_impl, AppState};
use grpc_lib::db::ConnectionFactory;
use grpc_lib::domain::identity::{
    CredentialStatus, IdentityCertificate, IdentitySigner, IdentityStorePort, SubjectType,
    SIGNATURE_VERSION_ED25519,
};
use grpc_lib::infrastructure::security::{AgeFileEncryptionProvider, Ed25519SigningProvider};
use grpc_lib::infrastructure::sync::packages::signing::Ed25519PackageSigner;
use grpc_lib::infrastructure::sync::{PackageBuilder, SerdeJsonSyncPackageSerializer};
use grpc_lib::models::{Product, ProductExportRow};
use grpc_lib::repositories::RepositoryProvider;

const WILAYA_CODE: &str = "16";
/// WILAYA anchor signing key — matches the seeded WILAYA certificate.
const WILAYA_SECRET: [u8; 32] = [9u8; 32];

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

/// `signature_version = 2` encrypted package written via `PackageBuilder`
/// (hash + Ed25519 signature computed inside the builder).
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

/// A V3 Product record with an explicit unit/factor tuple and TVA class.
#[allow(clippy::too_many_arguments)]
fn row_with_config(
    id: &str,
    name: &str,
    updated_at: DateTime<Utc>,
    created_at: DateTime<Utc>,
    purchase_unit: i32,
    consumption_unit: i32,
    conversion_factor: i32,
    tva_classification: i32,
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
        purchase_unit: Some(purchase_unit),
        consumption_unit: Some(consumption_unit),
        conversion_factor: Some(conversion_factor),
        tva_classification: Some(tva_classification),
    }
}

/// Conforming tuple `(1,1,1)` with TVA 0.
fn conforming_row(
    id: &str,
    name: &str,
    updated_at: DateTime<Utc>,
    created_at: DateTime<Utc>,
) -> ProductExportRow {
    row_with_config(id, name, updated_at, created_at, 1, 1, 1, 0)
}

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
            export_mode: None,
            target_node_id: None,
        },
        payload: ProductsExportDataset { product_rows: rows },
    }
}

/// Write a signed + encrypted products package and import it on the UNIT node.
fn import_package(
    state: &AppState,
    pkg: SyncPackage<ProductsExportDataset>,
) -> Result<grpc_lib::models::PackageImportResult, String> {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("products.sync");
    write_encrypted(&pkg, WILAYA_SECRET, &path);
    import_products_package_impl(state, path.to_string_lossy().into_owned())
}

// ─────────────────────────────────────────────────────────────────────────────
// Persisted-state inspection helpers
// ─────────────────────────────────────────────────────────────────────────────

/// (id, name, base_price_scaled, purchase_unit, consumption_unit,
/// conversion_factor, tva_classification) for every product row.
type ProductRowView = (
    String,
    String,
    i64,
    Option<i32>,
    Option<i32>,
    Option<i32>,
    Option<i32>,
);

fn product_rows(state: &AppState) -> Vec<ProductRowView> {
    let guard = state.get_db().expect("lock");
    let db = guard.as_ref().expect("db");
    let mut stmt = db
        .get_connection()
        .prepare(
            "SELECT id, name, base_price, purchase_unit, consumption_unit, conversion_factor, \
             tva_classification FROM products",
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
        ))
    })
    .expect("query")
    .map(|r| r.expect("row"))
    .collect()
}

/// (id, product_id, consumption_unit) for every inventory_stocks row.
fn stock_rows(state: &AppState, product_id: &str) -> Vec<(String, Option<i32>)> {
    let guard = state.get_db().expect("lock");
    let db = guard.as_ref().expect("db");
    let mut stmt = db
        .get_connection()
        .prepare(
            "SELECT id, consumption_unit FROM inventory_stocks WHERE product_id = ?1 ORDER BY id",
        )
        .expect("prepare");
    stmt.query_map(rusqlite::params![product_id], |row| {
        Ok((row.get(0)?, row.get(1)?))
    })
    .expect("query")
    .map(|r| r.expect("row"))
    .collect()
}

/// Insert one `stock_movements` row for the product — establishes the ADR-0058
/// freeze trigger `EXISTS(...)`. Direct SQL is test-only seeding.
fn insert_stock_movement(state: &AppState, product_id: &str, movement_id: &str) {
    let guard = state.get_db().expect("lock");
    let db = guard.as_ref().expect("db");
    db.get_connection()
        .execute(
            "INSERT INTO stock_movements \
             (id, product_id, movement_type, quantity, balance_before, balance_after, \
              reference_type, reference_id, notes, timestamp, user_id, username, unit_id, \
              updated_at, deleted, fiscal_year) \
             VALUES (?1, ?2, 'IN', 1000, 0, 1000, 'Order', 'ref-1', 'seed', ?3, 'u1', 'bob', \
              'u-1', ?3, 0, 2026)",
            rusqlite::params![movement_id, product_id, Utc::now().to_rfc3339()],
        )
        .expect("seed stock movement");
}

/// ADR-0058 rejection surfaces the Arabic user message (BusinessLogic errors
/// are NOT suffixed with dev details by `into_command_error`).
fn assert_frozen_rejection(err: &str, product_id: &str) {
    assert!(
        err.contains("لا يمكن تغيير وحدات المنتج") && err.contains(product_id),
        "unexpected error text: {err}"
    );
}

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

// ─────────────────────────────────────────────────────────────────────────────
// 1. No movement + unchanged tuple → accepted
// 2. No movement + changed tuple → accepted
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn no_movement_unchanged_tuple_accepted() {
    let (state, anchor_id) = build_unit_state();
    set_session(&state);
    let created = Utc::now();

    let first = import_package(
        &state,
        products_package_v3(
            "pkg-im-1",
            anchor_id,
            created,
            vec![conforming_row("p-1", "Flour", created, created)],
        ),
    )
    .expect("first conforming import ok");
    assert_eq!(first.added, 1);

    // A newer package with the SAME tuple (still no movement) is accepted.
    // `import_products_sync` only applies rows whose `updated_at` is strictly
    // newer than the persisted row, so the row timestamp must advance.
    let t2 = created + chrono::Duration::seconds(60);
    let second = import_package(
        &state,
        products_package_v3(
            "pkg-im-2",
            anchor_id,
            t2,
            vec![conforming_row("p-1", "Flour", t2, created)],
        ),
    )
    .expect("unchanged tuple re-import accepted without movement");
    assert_eq!(second.updated, 1);

    let rows = product_rows(&state);
    assert_eq!(rows.len(), 1);
    assert_eq!(
        (rows[0].3, rows[0].4, rows[0].5),
        (Some(1), Some(1), Some(1))
    );
}

#[test]
fn no_movement_changed_tuple_accepted() {
    let (state, anchor_id) = build_unit_state();
    set_session(&state);
    let created = Utc::now();

    import_package(
        &state,
        products_package_v3(
            "pkg-im-2a",
            anchor_id,
            created,
            vec![conforming_row("p-2", "Rice", created, created)],
        ),
    )
    .expect("first conforming import ok");

    // No stock movement yet → the tuple MAY change.
    let t2 = created + chrono::Duration::seconds(60);
    import_package(
        &state,
        products_package_v3(
            "pkg-im-2b",
            anchor_id,
            t2,
            vec![row_with_config("p-2", "Rice", t2, created, 2, 1, 1, 0)],
        ),
    )
    .expect("changed tuple accepted before stock activity");

    let rows = product_rows(&state);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].3, Some(2), "purchase_unit moved forward pre-freeze");
    assert_eq!(rows[0].4, Some(1));
    assert_eq!(rows[0].5, Some(1));
}

// ─────────────────────────────────────────────────────────────────────────────
// 3. Movement exists + unchanged tuple → accepted
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn movement_exists_unchanged_tuple_accepted() {
    let (state, anchor_id) = build_unit_state();
    set_session(&state);
    let created = Utc::now();

    import_package(
        &state,
        products_package_v3(
            "pkg-im-3a",
            anchor_id,
            created,
            vec![conforming_row("p-3", "Semolina", created, created)],
        ),
    )
    .expect("first import ok");
    insert_stock_movement(&state, "p-3", "mv-3");

    let t2 = created + chrono::Duration::seconds(60);
    let second = import_package(
        &state,
        products_package_v3(
            "pkg-im-3b",
            anchor_id,
            t2,
            vec![conforming_row("p-3", "Semolina", t2, created)],
        ),
    )
    .expect("unchanged tuple accepted with movement history");
    assert_eq!(second.updated, 1);

    let rows = product_rows(&state);
    assert_eq!(rows.len(), 1);
    assert_eq!(
        (rows[0].3, rows[0].4, rows[0].5),
        (Some(1), Some(1), Some(1))
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 4-6. Movement exists + changed tuple member → whole package rejected
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn movement_exists_changed_purchase_unit_whole_package_rejected() {
    let (state, anchor_id) = build_unit_state();
    set_session(&state);
    let created = Utc::now();

    import_package(
        &state,
        products_package_v3(
            "pkg-im-4a",
            anchor_id,
            created,
            vec![conforming_row("p-4", "Bread", created, created)],
        ),
    )
    .expect("first import ok");
    insert_stock_movement(&state, "p-4", "mv-4");

    let err = import_package(
        &state,
        products_package_v3(
            "pkg-im-4b",
            anchor_id,
            created,
            vec![row_with_config(
                "p-4", "Bread", created, created, 2, 1, 1, 0,
            )],
        ),
    )
    .expect_err("changed purchase_unit on frozen product must reject");
    assert_frozen_rejection(&err, "p-4");

    // Zero mutation, zero marker (ADR-0058 §5/§6).
    let rows = product_rows(&state);
    assert_eq!(rows.len(), 1, "no new Product row");
    assert_eq!(
        (rows[0].3, rows[0].4, rows[0].5),
        (Some(1), Some(1), Some(1)),
        "persisted tuple unchanged"
    );
    assert_eq!(rows[0].1, "Bread", "name unchanged");
    assert_eq!(applied_package_marker_count(&state, "pkg-im-4b"), 0);
}

#[test]
fn movement_exists_changed_consumption_unit_whole_package_rejected() {
    let (state, anchor_id) = build_unit_state();
    set_session(&state);
    let created = Utc::now();

    import_package(
        &state,
        products_package_v3(
            "pkg-im-5a",
            anchor_id,
            created,
            vec![conforming_row("p-5", "Butter", created, created)],
        ),
    )
    .expect("first import ok");
    insert_stock_movement(&state, "p-5", "mv-5");

    let err = import_package(
        &state,
        products_package_v3(
            "pkg-im-5b",
            anchor_id,
            created,
            vec![row_with_config(
                "p-5", "Butter", created, created, 1, 2, 1, 0,
            )],
        ),
    )
    .expect_err("changed consumption_unit on frozen product must reject");
    assert_frozen_rejection(&err, "p-5");

    let rows = product_rows(&state);
    assert_eq!(
        (rows[0].3, rows[0].4, rows[0].5),
        (Some(1), Some(1), Some(1))
    );
    assert_eq!(applied_package_marker_count(&state, "pkg-im-5b"), 0);
}

#[test]
fn movement_exists_changed_conversion_factor_whole_package_rejected() {
    let (state, anchor_id) = build_unit_state();
    set_session(&state);
    let created = Utc::now();

    // Mixed-unit product: purchase Kg (1) → consumption Piece (6) w/ factor 2.
    import_package(
        &state,
        products_package_v3(
            "pkg-im-6a",
            anchor_id,
            created,
            vec![row_with_config("p-6", "Eggs", created, created, 1, 6, 2, 0)],
        ),
    )
    .expect("first import ok");
    insert_stock_movement(&state, "p-6", "mv-6");

    let err = import_package(
        &state,
        products_package_v3(
            "pkg-im-6b",
            anchor_id,
            created,
            vec![row_with_config("p-6", "Eggs", created, created, 1, 6, 3, 0)],
        ),
    )
    .expect_err("changed conversion_factor on frozen product must reject");
    assert_frozen_rejection(&err, "p-6");

    let rows = product_rows(&state);
    assert_eq!(
        (rows[0].3, rows[0].4, rows[0].5),
        (Some(1), Some(6), Some(2))
    );
    assert_eq!(applied_package_marker_count(&state, "pkg-im-6b"), 0);
}

// ─────────────────────────────────────────────────────────────────────────────
// 7. Movement exists + changed name/base_price only → accepted
// 8. tva_classification NOT frozen by ADR-0058
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn movement_exists_changed_name_and_base_price_accepted() {
    let (state, anchor_id) = build_unit_state();
    set_session(&state);
    let t1 = Utc::now();
    let t2 = t1 + chrono::Duration::seconds(60);

    import_package(
        &state,
        products_package_v3(
            "pkg-im-7a",
            anchor_id,
            t1,
            vec![conforming_row("p-7", "OldName", t1, t1)],
        ),
    )
    .expect("first import ok");
    insert_stock_movement(&state, "p-7", "mv-7");

    let mut renamed = conforming_row("p-7", "NewName", t2, t1);
    renamed.product.base_price = 25.0;
    import_package(
        &state,
        products_package_v3("pkg-im-7b", anchor_id, t2, vec![renamed]),
    )
    .expect("name/base_price are NOT frozen — accepted with movement history");

    let rows = product_rows(&state);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].1, "NewName");
    assert_eq!(rows[0].2, 2500, "base_price (scaled) moved forward");
    assert_eq!(
        (rows[0].3, rows[0].4, rows[0].5),
        (Some(1), Some(1), Some(1)),
        "frozen tuple unchanged"
    );
}

#[test]
fn movement_exists_changed_tva_not_frozen_by_this_adr() {
    let (state, anchor_id) = build_unit_state();
    set_session(&state);
    let created = Utc::now();

    import_package(
        &state,
        products_package_v3(
            "pkg-im-8a",
            anchor_id,
            created,
            vec![row_with_config("p-8", "Milk", created, created, 1, 1, 1, 0)],
        ),
    )
    .expect("first import ok");
    insert_stock_movement(&state, "p-8", "mv-8");

    // ADR-0058 §4.3: tva_classification is governed by the existing fiscal-year
    // rule, NOT frozen by this ADR. A V3 re-import changing only the TVA class
    // is accepted.
    let t2 = created + chrono::Duration::seconds(60);
    import_package(
        &state,
        products_package_v3(
            "pkg-im-8b",
            anchor_id,
            t2,
            vec![row_with_config("p-8", "Milk", t2, created, 1, 1, 1, 2)],
        ),
    )
    .expect("tva_classification is NOT part of the frozen tuple");

    let rows = product_rows(&state);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].6, Some(2), "tva_classification moved forward");
    assert_eq!(
        (rows[0].3, rows[0].4, rows[0].5),
        (Some(1), Some(1), Some(1)),
        "frozen tuple unchanged"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 9-10. Forbidden tuple change → zero Product mutation AND zero inventory
//       mutation
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn forbidden_tuple_change_zero_product_and_inventory_mutation() {
    let (state, anchor_id) = build_unit_state();
    set_session(&state);
    let created = Utc::now();

    import_package(
        &state,
        products_package_v3(
            "pkg-im-9a",
            anchor_id,
            created,
            vec![conforming_row("p-9", "Yogurt", created, created)],
        ),
    )
    .expect("first import ok");
    let stock_before = stock_rows(&state, "p-9");
    assert_eq!(
        stock_before.len(),
        1,
        "keyed inventory identity established"
    );
    insert_stock_movement(&state, "p-9", "mv-9");

    let err = import_package(
        &state,
        products_package_v3(
            "pkg-im-9b",
            anchor_id,
            created,
            vec![row_with_config(
                "p-9", "Yogurt", created, created, 2, 1, 1, 0,
            )],
        ),
    )
    .expect_err("forbidden tuple change must reject the whole package");
    assert_frozen_rejection(&err, "p-9");

    // Zero Product mutation.
    let rows = product_rows(&state);
    assert_eq!(rows.len(), 1, "no new Product row");
    assert_eq!(
        (rows[0].3, rows[0].4, rows[0].5),
        (Some(1), Some(1), Some(1)),
        "Product tuple unchanged"
    );

    // Zero inventory mutation — same keyed identity, unchanged.
    let stock_after = stock_rows(&state, "p-9");
    assert_eq!(stock_after.len(), 1, "no stock row added/lost");
    assert_eq!(
        stock_before[0].0, stock_after[0].0,
        "stock identity preserved"
    );
    assert_eq!(stock_after[0].1, Some(1), "keyed identity unchanged");

    assert_eq!(applied_package_marker_count(&state, "pkg-im-9b"), 0);
}

// ─────────────────────────────────────────────────────────────────────────────
// 11. Multi-product package, one violating Product → entire package rejected
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn multi_product_single_violation_rejects_entire_package() {
    let (state, anchor_id) = build_unit_state();
    set_session(&state);
    let created = Utc::now();

    // Package A: two conforming products, both tuples (1,1,1).
    import_package(
        &state,
        products_package_v3(
            "pkg-im-11a",
            anchor_id,
            created,
            vec![
                conforming_row("p-11a", "Clean", created, created),
                conforming_row("p-11b", "FrozenTarget", created, created),
            ],
        ),
    )
    .expect("multi-product conforming import ok");

    // p-11b gains movement history; p-11a stays clean.
    insert_stock_movement(&state, "p-11b", "mv-11b");

    // Package B: p-11a keeps its tuple (structural change only), p-11b tries a
    // forbidden purchase_unit change. The entire package must be rejected.
    let mut clean_renamed = conforming_row("p-11a", "Clean Renamed", created, created);
    clean_renamed.product.base_price = 12.0;
    let err = import_package(
        &state,
        products_package_v3(
            "pkg-im-11b",
            anchor_id,
            created,
            vec![
                clean_renamed,
                row_with_config("p-11b", "FrozenTarget", created, created, 2, 1, 1, 0),
            ],
        ),
    )
    .expect_err("one violating Product rejects the ENTIRE package");
    assert_frozen_rejection(&err, "p-11b");

    // No Product changes applied — not even for the clean product.
    let rows = product_rows(&state);
    assert_eq!(rows.len(), 2, "both original rows intact");
    let clean = rows.iter().find(|r| r.0 == "p-11a").expect("clean row");
    assert_eq!(clean.1, "Clean", "clean product's name NOT applied");
    assert_eq!(clean.2, 1000, "clean product's base_price NOT applied");
    assert_eq!(
        (clean.3, clean.4, clean.5),
        (Some(1), Some(1), Some(1)),
        "clean product tuple unchanged"
    );
    let frozen = rows.iter().find(|r| r.0 == "p-11b").expect("frozen row");
    assert_eq!(
        (frozen.3, frozen.4, frozen.5),
        (Some(1), Some(1), Some(1)),
        "frozen product tuple unchanged"
    );
    assert_eq!(applied_package_marker_count(&state, "pkg-im-11b"), 0);
}

// ─────────────────────────────────────────────────────────────────────────────
// 12. Corrected / re-exported conforming package → subsequently accepted
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn corrected_conforming_package_subsequently_accepted() {
    let (state, anchor_id) = build_unit_state();
    set_session(&state);
    let created = Utc::now();

    import_package(
        &state,
        products_package_v3(
            "pkg-im-12a",
            anchor_id,
            created,
            vec![conforming_row("p-12", "OliveOil", created, created)],
        ),
    )
    .expect("first import ok");
    insert_stock_movement(&state, "p-12", "mv-12");

    // Forbidden change rejected...
    let err = import_package(
        &state,
        products_package_v3(
            "pkg-im-12b",
            anchor_id,
            created,
            vec![row_with_config(
                "p-12", "OliveOil", created, created, 2, 1, 1, 0,
            )],
        ),
    )
    .expect_err("forbidden tuple change rejected");
    assert_frozen_rejection(&err, "p-12");

    // ...and the corrected (conforming) re-export is accepted.
    import_package(
        &state,
        products_package_v3(
            "pkg-im-12c",
            anchor_id,
            created,
            vec![conforming_row("p-12", "OliveOil", created, created)],
        ),
    )
    .expect("corrected conforming package imported normally");

    let rows = product_rows(&state);
    assert_eq!(rows.len(), 1);
    assert_eq!(
        (rows[0].3, rows[0].4, rows[0].5),
        (Some(1), Some(1), Some(1)),
        "corrected tuple accepted after remediation"
    );
    assert_eq!(applied_package_marker_count(&state, "pkg-im-12c"), 1);
}
