//! SYNC-005 (ADR-0046, DESIGN B) integration tests: UNIT → WILAYA data sync.
//!
//! ADR-0046 §3 (I1-I5) / §5 (verification order) / §11 (testing contract).
//!
//! Exercises the REAL import pipeline end-to-end: WILAYA `AppState` + Admin
//! session, encrypted `.sync` package files, real Ed25519/Age crypto, and the
//! `*_impl` command entry points (`run_import_pipeline` inside).
//!
//! The trust-store fixture is a documented, controlled arrangement: a WILAYA
//! anchor certificate (the fleet trust root representative) whose issued UNIT
//! certificate is pre-installed in the Identity Store with
//! `issuer_identity_id = Some(anchor)`. The pipeline itself does not require
//! the anchor — membership and binding resolve exclusively against trusted
//! local persisted state (Identity Store certificate + `units` row) per
//! ADR-0046 §3 I3.
//!
//! Security invariant under test (ADR-0046 §3 I4): a UNIT signs ONLY the three
//! data kinds (`stock_movements` / `daily_report` / `monthly_summary`), the
//! WILAYA importer is the ONLY consumer, and the issuer's authenticated
//! `subject_id` is the authoritative import target + payload unit binding.

#[allow(dead_code)]
mod common;

use chrono::{NaiveDate, Utc};
use std::path::Path;
use uuid::Uuid;

use grpc_lib::application::services::SyncPackageIdentityVerificationService;
use grpc_lib::application::sync::{
    PackageId, SchemaVersion, SyncPackage, SyncPackageMetadata,
};
use grpc_lib::application::usecases::exports::types::{
    DailyReportExportDataset, MonthlySummaryExportDataset, ProductsExportDataset,
    StockMovementsExportDataset,
};
use grpc_lib::commands::{
    import_daily_report_package_impl, import_identity_access_package_impl,
    import_monthly_summary_package_impl, import_products_package_impl, import_registry_package_impl,
    import_stock_movements_package_impl, import_trust_package_impl, AppState,
};
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
use grpc_lib::models::{
    DailyConsumptionSyncLine, DailyReportSyncSnapshot, DailyDetailSyncSnapshot, IdentityAccessPayload,
    MealSectionSyncSnapshot, MealType, MonthlySummary, Product, ProductExportRow,
    StockMovement, StockMovementType,
};
use grpc_lib::repositories::RepositoryProvider;

const WILAYA_CODE: &str = "16";

/// UNIT issuer signing key — matches the seeded UNIT certificate public key.
const UNIT_SECRET: [u8; 32] = [42u8; 32];
/// WILAYA anchor signing key — matches the seeded WILAYA certificate.
const WILAYA_SECRET: [u8; 32] = [9u8; 32];
/// A DIFFERENT key: used to prove that signature validity ≠ identity binding.
const OTHER_SECRET: [u8; 32] = [7u8; 32];

/// WILAYA node fixture: settings (WILAYA/16/configured), open fiscal year 2026,
/// and the WILAYA anchor certificate (trust-store fixture root). Returns the
/// anchor identity id so UNIT fixtures can reference it as issuer.
fn build_wilaya_state() -> (AppState, Uuid) {
    let db = ConnectionFactory::new_for_test().expect("db");
    let state = AppState::new_for_test(db);
    {
        let guard = state.get_db().expect("lock");
        let db = guard.as_ref().expect("db");
        db.get_connection()
            .execute(
                "UPDATE settings SET node_type = 'WILAYA', configured = 1, wilaya_code = '16', wilaya_name = 'Alger' WHERE id = 1",
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

/// UNIT node fixture: settings (UNIT/16/configured with the local unit
/// identity), open fiscal year 2026, and the WILAYA anchor certificate (the
/// trust anchor a provisioned UNIT node holds). Returns the local unit id so
/// tests can seed the node's own ACTIVE UNIT certificate.
fn build_unit_state() -> (AppState, Uuid, Uuid) {
    let db = ConnectionFactory::new_for_test().expect("db");
    let state = AppState::new_for_test(db);
    let unit_id = Uuid::new_v4();
    {
        let guard = state.get_db().expect("lock");
        let db = guard.as_ref().expect("db");
        db.get_connection()
            .execute(
                "UPDATE settings SET node_type = 'UNIT', configured = 1, wilaya_code = '16', wilaya_name = 'Alger', unit_name = ?1 WHERE id = 1",
                rusqlite::params![unit_id.to_string()],
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
    (state, anchor_id, unit_id)
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

/// UNIT fixture: local `units` row of the importer's WILAYA + ACTIVE UNIT
/// certificate in the Identity Store, issued by the WILAYA anchor
/// (issuer_identity_id = Some(anchor_id), ADR-0046 trust-store fixture).
fn seed_unit(
    state: &AppState,
    identity_id: Uuid,
    unit_id: Uuid,
    status: CredentialStatus,
    anchor_id: Uuid,
    wilaya_code: &str,
) {
    let now = Utc::now().to_rfc3339();
    {
        let guard = state.get_db().expect("lock");
        let db = guard.as_ref().expect("db");
        db.get_connection()
            .execute(
                "INSERT OR REPLACE INTO units (id, code, name, wilaya_code, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![unit_id.to_string(), format!("U{unit_id}"), format!("Unit {unit_id}"), wilaya_code, now],
            )
            .expect("unit row");
    }
    let certificate = IdentityCertificate {
        identity_id,
        subject_type: SubjectType::Unit,
        subject_id: unit_id,
        issuer_identity_id: Some(anchor_id),
        credential_id: Uuid::new_v4(),
        generation: 1,
        status,
        public_key: Ed25519SigningProvider::new(UNIT_SECRET).public_key(),
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
            .expect("seed unit certificate");
    }
}

fn set_wilaya_admin_session(state: &AppState) {
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

fn v2_metadata(pkg_id: &str, issuer_id: Uuid, source_node_id: &str, sequence: u64) -> SyncPackageMetadata {
    let signer = Ed25519PackageSigner::new(UNIT_SECRET);
    SyncPackageMetadata {
        schema_version: SchemaVersion::V2,
        created_at: Utc::now(),
        source_node_id: source_node_id.to_string(),
        package_sequence: Some(sequence),
        issuer_identity_id: Some(issuer_id),
        package_id: PackageId(pkg_id.to_string()),
        signature_version: Some(SIGNATURE_VERSION_ED25519),
        signing_key_id: Some(signer.public_key_hex()),
        integrity_hash: None,
        signature: None,
    }
}

fn movement(id: &str, product_id: &str, unit_id: Option<&str>, in_out: StockMovementType) -> StockMovement {
    StockMovement {
        id: id.to_string(),
        product_id: product_id.to_string(),
        product_name: Some("Bread".into()),
        movement_type: in_out,
        quantity: 5.0,
        balance_before: 10.0,
        balance_after: 15.0,
        reference_type: None,
        reference_id: None,
        notes: None,
        timestamp: Utc::now().to_rfc3339(),
        user_id: "unit-admin".to_string(),
        username: "unit-admin".to_string(),
        unit_id: unit_id.map(|s| s.to_string()),
        fiscal_year: Some(2026),
        unit_cost: Some(20.0),
    }
}

fn stock_movements_package(
    pkg_id: &str,
    issuer_id: Uuid,
    unit_id: Uuid,
    sequence: u64,
    movements: Vec<StockMovement>,
) -> SyncPackage<StockMovementsExportDataset> {
    SyncPackage {
        metadata: v2_metadata(pkg_id, issuer_id, &unit_id.to_string(), sequence),
        payload: StockMovementsExportDataset { movements },
    }
}

fn daily_report_package(
    pkg_id: &str,
    issuer_id: Uuid,
    unit_id: Uuid,
    sequence: u64,
) -> SyncPackage<DailyReportExportDataset> {
    SyncPackage {
        metadata: v2_metadata(pkg_id, issuer_id, &unit_id.to_string(), sequence),
        payload: DailyReportExportDataset {
            snapshot: DailyReportSyncSnapshot {
                report_id: format!("rep-{pkg_id}"),
                date: NaiveDate::from_ymd_opt(2026, 8, 10).unwrap(),
                total_daily_cost: 120.0,
                total_daily_average: 12.0,
                total_daily_beneficiaries: 10,
                meals: vec![MealSectionSyncSnapshot {
                    meal_type: MealType::Breakfast,
                    staff_24h_count: 5,
                    staff_8h_count: 3,
                    reservation_count: 0,
                    mission_count: 0,
                    guest_count: 2,
                    total_beneficiaries: 10,
                    total_meal_cost: 120.0,
                    meal_average: 12.0,
                    items: vec![DailyConsumptionSyncLine {
                        product_id: "prod-1".into(),
                        product_name: "Bread".into(),
                        quantity: 1.0,
                        unit_price: 20.0,
                        total_cost: 20.0,
                    }],
                }],
            },
        },
    }
}

fn monthly_summary_package(
    pkg_id: &str,
    issuer_id: Uuid,
    unit_id: Uuid,
    sequence: u64,
) -> SyncPackage<MonthlySummaryExportDataset> {
    SyncPackage {
        metadata: v2_metadata(pkg_id, issuer_id, &unit_id.to_string(), sequence),
        payload: MonthlySummaryExportDataset {
            summary: MonthlySummary {
                month: 8,
                year: 2026,
                total_beneficiaries: 10,
                total_consumption_value: 1200.0,
                breakfast_average: 12.0,
                lunch_average: 12.0,
                dinner_average: 12.0,
                daily_average: 12.0,
                report_count: 1,
            },
            daily_detail_rows: vec![DailyDetailSyncSnapshot {
                date: NaiveDate::from_ymd_opt(2026, 8, 10).unwrap(),
                total_daily_beneficiaries: 10,
                total_daily_cost: 120.0,
                breakfast_average: 12.0,
                lunch_average: 12.0,
                dinner_average: 12.0,
                daily_average: 12.0,
            }],
        },
    }
}

fn products_package(
    pkg_id: &str,
    issuer_id: Uuid,
    unit_id: Uuid,
    sequence: u64,
) -> SyncPackage<ProductsExportDataset> {
    SyncPackage {
        metadata: v2_metadata(pkg_id, issuer_id, &unit_id.to_string(), sequence),
        payload: ProductsExportDataset {
            product_rows: vec![ProductExportRow {
                product: Product {
                    id: "prod-1".into(),
                    name: "Bread".into(),
                    base_price: 20.0,
                    tva: 0.0,
                    supplier_name: None,
                    year: 2026,
                    created_at: Utc::now(),
                },
                updated_at: Utc::now().to_rfc3339(),
                node_id: unit_id.to_string(),
                deleted: 0,
            }],
        },
    }
}

fn identity_access_package(
    pkg_id: &str,
    issuer_id: Uuid,
    unit_id: Uuid,
    sequence: u64,
) -> SyncPackage<IdentityAccessPayload> {
    SyncPackage {
        metadata: v2_metadata(pkg_id, issuer_id, &unit_id.to_string(), sequence),
        payload: IdentityAccessPayload {
            unit_code: format!("U{unit_id}"),
            admin_password_hash: "x".into(),
            admin_enabled: true,
            user_password_hash: "x".into(),
            user_enabled: true,
        },
    }
}

fn trust_package(
    pkg_id: &str,
    issuer_id: Uuid,
    unit_id: Uuid,
    sequence: u64,
) -> SyncPackage<grpc_lib::application::usecases::sync::import_trust_package::TrustPackagePayload> {
    use grpc_lib::application::usecases::sync::import_trust_package::TrustPackagePayload;
    SyncPackage {
        metadata: v2_metadata(pkg_id, issuer_id, &unit_id.to_string(), sequence),
        payload: TrustPackagePayload {
            certificates: vec![],
            revocations: vec![],
        },
    }
}

fn registry_package(
    pkg_id: &str,
    issuer_id: Uuid,
    unit_id: Uuid,
    sequence: u64,
) -> SyncPackage<grpc_lib::application::usecases::sync::import_registry_package::RegistryPackagePayload> {
    use grpc_lib::application::usecases::sync::import_registry_package::RegistryPackagePayload;
    SyncPackage {
        metadata: v2_metadata(pkg_id, issuer_id, &unit_id.to_string(), sequence),
        payload: RegistryPackagePayload {
            snapshot_version: 1,
            wilaya_identity_id: issuer_id,
            units: vec![],
        },
    }
}

fn last_applied(state: &AppState, issuer: &str) -> Option<u64> {
    let guard = state.get_db().expect("lock");
    let db = guard.as_ref().expect("db");
    SyncPackageIdentityVerificationService::last_applied_sequence(db.executor(), issuer)
        .expect("read ledger")
}

fn movement_rows(state: &AppState, unit_id: &str) -> Vec<(String, String, String)> {
    let guard = state.get_db().expect("lock");
    let db = guard.as_ref().expect("db");
    let mut stmt = db
        .get_connection()
        .prepare("SELECT id, unit_id, movement_type FROM stock_movements WHERE unit_id = ?1")
        .expect("prepare");
    stmt.query_map(rusqlite::params![unit_id], |row| {
        Ok((row.get(0)?, row.get(1)?, row.get(2)?))
    })
    .expect("query")
    .map(|r| r.expect("row"))
    .collect()
}

// ─────────────────────────────────────────────────────────────────────────────
// Positive: UNIT → WILAYA data kinds (ADR-0046 I1/I2)
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn unit_stock_movements_imported_on_wilaya_with_sequence_1() {
    let (state, anchor_id) = build_wilaya_state();
    let unit_identity = Uuid::new_v4();
    let unit_id = Uuid::new_v4();
    seed_unit(&state, unit_identity, unit_id, CredentialStatus::Active, anchor_id, WILAYA_CODE);
    set_wilaya_admin_session(&state);
    let product_id = common::create_test_product(&state, "Bread", 20.0, 2026);

    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("mov.sync");
    let package = sign_v2_package(
        stock_movements_package(
            "sm-pkg-1",
            unit_identity,
            unit_id,
            1,
            vec![
                movement("m1", &product_id, Some(&unit_id.to_string()), StockMovementType::In),
                movement("m2", &product_id, Some(&unit_id.to_string()), StockMovementType::Out),
            ],
        ),
        UNIT_SECRET,
    );
    write_encrypted(&package, UNIT_SECRET, &path);

    let result = import_stock_movements_package_impl(
        &state,
        path.to_string_lossy().into_owned(),
        unit_id.to_string(),
    )
    .expect("UNIT stock movements accepted on WILAYA");
    assert_eq!(result.movement_count, 2);
    assert_eq!(last_applied(&state, &unit_identity.to_string()), Some(1));

    let rows = movement_rows(&state, &unit_id.to_string());
    assert_eq!(rows.len(), 2, "movements persisted under the authenticated unit");
    for (_, stored_unit, _) in rows {
        assert_eq!(stored_unit, unit_id.to_string());
    }
}

#[test]
fn unit_stock_movements_sequence_2_accepted_after_sequence_1() {
    let (state, anchor_id) = build_wilaya_state();
    let unit_identity = Uuid::new_v4();
    let unit_id = Uuid::new_v4();
    seed_unit(&state, unit_identity, unit_id, CredentialStatus::Active, anchor_id, WILAYA_CODE);
    set_wilaya_admin_session(&state);
    let product_id = common::create_test_product(&state, "Bread", 20.0, 2026);

    let dir = tempfile::tempdir().expect("temp dir");
    let first = sign_v2_package(
        stock_movements_package(
            "sm-pkg-1",
            unit_identity,
            unit_id,
            1,
            vec![movement("m1", &product_id, Some(&unit_id.to_string()), StockMovementType::In)],
        ),
        UNIT_SECRET,
    );
    write_encrypted(&first, UNIT_SECRET, &dir.path().join("first.sync"));
    import_stock_movements_package_impl(
        &state,
        dir.path().join("first.sync").to_string_lossy().into_owned(),
        unit_id.to_string(),
    )
    .expect("sequence 1 accepted");

    let second = sign_v2_package(
        stock_movements_package(
            "sm-pkg-2",
            unit_identity,
            unit_id,
            2,
            vec![movement("m2", &product_id, Some(&unit_id.to_string()), StockMovementType::Out)],
        ),
        UNIT_SECRET,
    );
    write_encrypted(&second, UNIT_SECRET, &dir.path().join("second.sync"));
    import_stock_movements_package_impl(
        &state,
        dir.path().join("second.sync").to_string_lossy().into_owned(),
        unit_id.to_string(),
    )
    .expect("sequence 2 accepted");
    assert_eq!(last_applied(&state, &unit_identity.to_string()), Some(2));
}

#[test]
fn unit_movements_without_unit_id_are_restamped_to_authenticated_subject() {
    let (state, anchor_id) = build_wilaya_state();
    let unit_identity = Uuid::new_v4();
    let unit_id = Uuid::new_v4();
    seed_unit(&state, unit_identity, unit_id, CredentialStatus::Active, anchor_id, WILAYA_CODE);
    set_wilaya_admin_session(&state);
    let product_id = common::create_test_product(&state, "Bread", 20.0, 2026);

    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("mov.sync");
    // OUT allows a NULL unit_id at the schema level; the signed payload carries
    // no unit_id — the mutation restamp (authoritative source = the
    // authenticated subject) must bind it.
    let package = sign_v2_package(
        stock_movements_package(
            "sm-pkg-restamp",
            unit_identity,
            unit_id,
            1,
            vec![movement("m1", &product_id, None, StockMovementType::Out)],
        ),
        UNIT_SECRET,
    );
    write_encrypted(&package, UNIT_SECRET, &path);

    import_stock_movements_package_impl(
        &state,
        path.to_string_lossy().into_owned(),
        unit_id.to_string(),
    )
    .expect("absent unit_id restamps from authenticated subject");

    let rows = movement_rows(&state, &unit_id.to_string());
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].1, unit_id.to_string());
}

#[test]
fn unit_daily_report_imported_on_wilaya() {
    let (state, anchor_id) = build_wilaya_state();
    let unit_identity = Uuid::new_v4();
    let unit_id = Uuid::new_v4();
    seed_unit(&state, unit_identity, unit_id, CredentialStatus::Active, anchor_id, WILAYA_CODE);
    set_wilaya_admin_session(&state);

    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("daily.sync");
    let package = sign_v2_package(
        daily_report_package("dr-pkg-1", unit_identity, unit_id, 1),
        UNIT_SECRET,
    );
    write_encrypted(&package, UNIT_SECRET, &path);

    let result = import_daily_report_package_impl(
        &state,
        path.to_string_lossy().into_owned(),
        unit_id.to_string(),
    )
    .expect("UNIT daily report accepted on WILAYA");
    assert_eq!(result.report_count, 1);
    assert_eq!(last_applied(&state, &unit_identity.to_string()), Some(1));
}

#[test]
fn unit_monthly_summary_imported_on_wilaya() {
    let (state, anchor_id) = build_wilaya_state();
    let unit_identity = Uuid::new_v4();
    let unit_id = Uuid::new_v4();
    seed_unit(&state, unit_identity, unit_id, CredentialStatus::Active, anchor_id, WILAYA_CODE);
    set_wilaya_admin_session(&state);

    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("monthly.sync");
    let package = sign_v2_package(
        monthly_summary_package("ms-pkg-1", unit_identity, unit_id, 1),
        UNIT_SECRET,
    );
    write_encrypted(&package, UNIT_SECRET, &path);

    let result = import_monthly_summary_package_impl(
        &state,
        path.to_string_lossy().into_owned(),
        unit_id.to_string(),
    )
    .expect("UNIT monthly summary accepted on WILAYA");
    assert_eq!(result.report_count, 1);
    assert_eq!(last_applied(&state, &unit_identity.to_string()), Some(1));
}

#[test]
fn unit_issuer_cross_kind_sequence_continuity_holds() {
    // Same authenticated UNIT issuer, different data kinds: the per-issuer
    // transport ledger is sequence-continuous across kinds.
    let (state, anchor_id) = build_wilaya_state();
    let unit_identity = Uuid::new_v4();
    let unit_id = Uuid::new_v4();
    seed_unit(&state, unit_identity, unit_id, CredentialStatus::Active, anchor_id, WILAYA_CODE);
    set_wilaya_admin_session(&state);
    let product_id = common::create_test_product(&state, "Bread", 20.0, 2026);

    let dir = tempfile::tempdir().expect("temp dir");
    let first = sign_v2_package(
        stock_movements_package(
            "sm-pkg-1",
            unit_identity,
            unit_id,
            1,
            vec![movement("m1", &product_id, Some(&unit_id.to_string()), StockMovementType::In)],
        ),
        UNIT_SECRET,
    );
    write_encrypted(&first, UNIT_SECRET, &dir.path().join("first.sync"));
    import_stock_movements_package_impl(
        &state,
        dir.path().join("first.sync").to_string_lossy().into_owned(),
        unit_id.to_string(),
    )
    .expect("stock movements seq 1");

    let second = sign_v2_package(
        daily_report_package("dr-pkg-2", unit_identity, unit_id, 2),
        UNIT_SECRET,
    );
    write_encrypted(&second, UNIT_SECRET, &dir.path().join("second.sync"));
    import_daily_report_package_impl(
        &state,
        dir.path().join("second.sync").to_string_lossy().into_owned(),
        unit_id.to_string(),
    )
    .expect("daily report seq 2 with same issuer");
    assert_eq!(last_applied(&state, &unit_identity.to_string()), Some(2));
}

#[test]
fn wilaya_issued_data_package_still_accepted() {
    // Regression: the WILAYA-issuer path of the import pipeline is untouched.
    let (state, anchor_uuid) = build_wilaya_state();
    set_wilaya_admin_session(&state);
    let unit_id = Uuid::new_v4();
    {
        let now = Utc::now().to_rfc3339();
        let guard = state.get_db().expect("lock");
        let db = guard.as_ref().expect("db");
        db.get_connection()
            .execute(
                "INSERT INTO units (id, code, name, wilaya_code, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![unit_id.to_string(), "U16X", "Wilaya-seeded unit", WILAYA_CODE, now],
            )
            .expect("unit row");
    }
    // WILAYA anchor as package issuer (source_node_id = wilaya code).
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("wilaya-daily.sync");
    let mut package = daily_report_package("dr-wilaya-1", anchor_uuid, unit_id, 1);
    package.metadata.source_node_id = WILAYA_CODE.to_string();
    let package = sign_v2_package(package, WILAYA_SECRET);
    write_encrypted(&package, WILAYA_SECRET, &path);

    import_daily_report_package_impl(
        &state,
        path.to_string_lossy().into_owned(),
        unit_id.to_string(),
    )
    .expect("WILAYA-issued daily report still accepted");
    assert_eq!(last_applied(&state, &anchor_uuid.to_string()), Some(1));
}

// ─────────────────────────────────────────────────────────────────────────────
// Negative: UNIT issuer authenticity, status, membership, binding (ADR-0046 I4)
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn unit_issuer_rejected_when_certificate_revoked() {
    let (state, anchor_id) = build_wilaya_state();
    let unit_identity = Uuid::new_v4();
    let unit_id = Uuid::new_v4();
    seed_unit(&state, unit_identity, unit_id, CredentialStatus::Revoked, anchor_id, WILAYA_CODE);
    set_wilaya_admin_session(&state);

    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("mov.sync");
    let package = sign_v2_package(
        stock_movements_package("sm-revoked", unit_identity, unit_id, 1, vec![]),
        UNIT_SECRET,
    );
    write_encrypted(&package, UNIT_SECRET, &path);

    let err = import_stock_movements_package_impl(
        &state,
        path.to_string_lossy().into_owned(),
        unit_id.to_string(),
    )
    .expect_err("revoked UNIT issuer must be rejected");
    assert!(err.contains("غير نشط"), "got: {err}");
    assert_eq!(last_applied(&state, &unit_identity.to_string()), None);
}

#[test]
fn unit_issuer_rejected_when_certificate_superseded() {
    let (state, anchor_id) = build_wilaya_state();
    let unit_identity = Uuid::new_v4();
    let unit_id = Uuid::new_v4();
    seed_unit(&state, unit_identity, unit_id, CredentialStatus::Superseded, anchor_id, WILAYA_CODE);
    set_wilaya_admin_session(&state);

    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("daily.sync");
    let package = sign_v2_package(
        daily_report_package("dr-superseded", unit_identity, unit_id, 1),
        UNIT_SECRET,
    );
    write_encrypted(&package, UNIT_SECRET, &path);

    let err = import_daily_report_package_impl(
        &state,
        path.to_string_lossy().into_owned(),
        unit_id.to_string(),
    )
    .expect_err("superseded UNIT issuer must be rejected");
    assert!(err.contains("غير نشط"), "got: {err}");
    assert_eq!(last_applied(&state, &unit_identity.to_string()), None);
}

#[test]
fn unit_issuer_rejected_when_certificate_expired() {
    let (state, anchor_id) = build_wilaya_state();
    let unit_identity = Uuid::new_v4();
    let unit_id = Uuid::new_v4();
    seed_unit(&state, unit_identity, unit_id, CredentialStatus::Active, anchor_id, WILAYA_CODE);
    {
        let now = Utc::now().to_rfc3339();
        let expired = (Utc::now() - chrono::Duration::days(1)).to_rfc3339();
        let guard = state.get_db().expect("lock");
        let db = guard.as_ref().expect("db");
        db.get_connection()
            .execute(
                "UPDATE identity_store SET not_after = ?1, updated_at = ?2 WHERE identity_id = ?3",
                rusqlite::params![expired, now, unit_identity.to_string()],
            )
            .expect("expire certificate");
    }
    set_wilaya_admin_session(&state);

    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("monthly.sync");
    let package = sign_v2_package(
        monthly_summary_package("ms-expired", unit_identity, unit_id, 1),
        UNIT_SECRET,
    );
    write_encrypted(&package, UNIT_SECRET, &path);

    let err = import_monthly_summary_package_impl(
        &state,
        path.to_string_lossy().into_owned(),
        unit_id.to_string(),
    )
    .expect_err("expired UNIT issuer must be rejected");
    assert!(err.contains("منتهية الصلاحية"), "got: {err}");
    assert_eq!(last_applied(&state, &unit_identity.to_string()), None);
}

#[test]
fn unit_issuer_rejected_when_certificate_unknown() {
    let (state, _anchor_id) = build_wilaya_state();
    let unknown_identity = Uuid::new_v4();
    let unit_id = Uuid::new_v4();
    // No certificate seeded for `unknown_identity`.
    set_wilaya_admin_session(&state);

    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("mov.sync");
    let package = sign_v2_package(
        stock_movements_package("sm-unknown", unknown_identity, unit_id, 1, vec![]),
        UNIT_SECRET,
    );
    write_encrypted(&package, UNIT_SECRET, &path);

    let err = import_stock_movements_package_impl(
        &state,
        path.to_string_lossy().into_owned(),
        unit_id.to_string(),
    )
    .expect_err("unknown UNIT issuer must be rejected");
    assert!(err.contains("غير موجود"), "got: {err}");
    assert_eq!(last_applied(&state, &unknown_identity.to_string()), None);
}

#[test]
fn unit_issuer_rejected_when_unit_belongs_to_foreign_wilaya() {
    let (state, anchor_id) = build_wilaya_state();
    let unit_identity = Uuid::new_v4();
    let unit_id = Uuid::new_v4();
    // Unit row of ANOTHER wilaya (10) — membership must fail against 16.
    seed_unit(&state, unit_identity, unit_id, CredentialStatus::Active, anchor_id, "10");
    set_wilaya_admin_session(&state);

    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("mov.sync");
    let package = sign_v2_package(
        stock_movements_package("sm-foreign", unit_identity, unit_id, 1, vec![]),
        UNIT_SECRET,
    );
    write_encrypted(&package, UNIT_SECRET, &path);

    let err = import_stock_movements_package_impl(
        &state,
        path.to_string_lossy().into_owned(),
        unit_id.to_string(),
    )
    .expect_err("foreign-wilaya UNIT issuer must be rejected");
    assert!(err.contains("لا تنتمي إلى ولاية المستورد"), "got: {err}");
    assert_eq!(last_applied(&state, &unit_identity.to_string()), None);
}

#[test]
fn unit_issuer_rejected_when_import_target_mismatches_authenticated_subject() {
    let (state, anchor_id) = build_wilaya_state();
    let unit_identity = Uuid::new_v4();
    let unit_id = Uuid::new_v4();
    seed_unit(&state, unit_identity, unit_id, CredentialStatus::Active, anchor_id, WILAYA_CODE);
    set_wilaya_admin_session(&state);

    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("mov.sync");
    let package = sign_v2_package(
        stock_movements_package("sm-mismatch", unit_identity, unit_id, 1, vec![]),
        UNIT_SECRET,
    );
    write_encrypted(&package, UNIT_SECRET, &path);

    let err = import_stock_movements_package_impl(
        &state,
        path.to_string_lossy().into_owned(),
        Uuid::new_v4().to_string(),
    )
    .expect_err("renderer-selected unit different from authenticated subject must be rejected");
    assert!(err.contains("لا تطابق هوية المُصدِر الموثّقة"), "got: {err}");
    assert_eq!(last_applied(&state, &unit_identity.to_string()), None);
}

#[test]
fn unit_issuer_rejected_when_payload_movement_belongs_to_another_unit() {
    let (state, anchor_id) = build_wilaya_state();
    let unit_identity = Uuid::new_v4();
    let unit_id = Uuid::new_v4();
    seed_unit(&state, unit_identity, unit_id, CredentialStatus::Active, anchor_id, WILAYA_CODE);
    set_wilaya_admin_session(&state);
    let product_id = common::create_test_product(&state, "Bread", 20.0, 2026);

    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("mov.sync");
    let foreign_unit = Uuid::new_v4().to_string();
    let package = sign_v2_package(
        stock_movements_package(
            "sm-wrong-movement-unit",
            unit_identity,
            unit_id,
            1,
            vec![movement("m1", &product_id, Some(&foreign_unit), StockMovementType::Out)],
        ),
        UNIT_SECRET,
    );
    write_encrypted(&package, UNIT_SECRET, &path);

    let err = import_stock_movements_package_impl(
        &state,
        path.to_string_lossy().into_owned(),
        unit_id.to_string(),
    )
    .expect_err("signed movement of another unit must be rejected");
    assert!(err.contains("وحدة بيانات الحزمة"), "got: {err}");
    assert_eq!(last_applied(&state, &unit_identity.to_string()), None);
}

#[test]
fn unit_issuer_rejected_when_signature_forged_with_other_key() {
    let (state, anchor_id) = build_wilaya_state();
    let unit_identity = Uuid::new_v4();
    let unit_id = Uuid::new_v4();
    seed_unit(&state, unit_identity, unit_id, CredentialStatus::Active, anchor_id, WILAYA_CODE);
    set_wilaya_admin_session(&state);

    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("mov.sync");
    // Package claims the UNIT issuer but is signed with OTHER_SECRET — whose
    // key does not match the issuer certificate's UNIT_SECRET key.
    let package = sign_v2_package(
        stock_movements_package("sm-forged", unit_identity, unit_id, 1, vec![]),
        OTHER_SECRET,
    );
    write_encrypted(&package, OTHER_SECRET, &path);

    let err = import_stock_movements_package_impl(
        &state,
        path.to_string_lossy().into_owned(),
        unit_id.to_string(),
    )
    .expect_err("forged signature must be rejected");
    assert!(err.contains("فشل التحقق"), "got: {err}");
    assert_eq!(last_applied(&state, &unit_identity.to_string()), None);
}

#[test]
fn unit_package_with_tampered_payload_and_refreshed_hash_rejected() {
    let (state, anchor_id) = build_wilaya_state();
    let unit_identity = Uuid::new_v4();
    let unit_id = Uuid::new_v4();
    seed_unit(&state, unit_identity, unit_id, CredentialStatus::Active, anchor_id, WILAYA_CODE);
    set_wilaya_admin_session(&state);

    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("mov.sync");
    let mut package = sign_v2_package(
        stock_movements_package("sm-tamper", unit_identity, unit_id, 1, vec![]),
        UNIT_SECRET,
    );
    // Attacker swaps the payload and recomputes the SHA-256 integrity hash
    // (public knowledge) — but cannot forge the Ed25519 signature. The file is
    // serialized + encrypted directly (bypassing PackageBuilder, which would
    // re-sign and thereby re-validate the tampered payload).
    package.payload = StockMovementsExportDataset {
        movements: vec![movement("m-evil", "prod-evil", Some(&unit_id.to_string()), StockMovementType::Out)],
    };
    let value = serde_json::to_value(&package).expect("value");
    let hash = Sha256PackageHasher
        .hash(&canonical_bytes_for_integrity(&value).expect("canonical"))
        .expect("hash");
    package.metadata.integrity_hash = Some(hash);
    {
        let plaintext = dir.path().join("tampered.json");
        let json = serde_json::to_vec(&package).expect("serialize tampered package");
        std::fs::write(&plaintext, &json).expect("write plaintext");
        let mut input = std::io::Cursor::new(json);
        let output = std::fs::File::create(&path).expect("create encrypted file");
        let mut output = std::io::BufWriter::new(output);
        AgeFileEncryptionProvider::new()
            .encrypt_stream(&mut input, &mut output)
            .expect("encrypt tampered package");
    }

    let err = import_stock_movements_package_impl(
        &state,
        path.to_string_lossy().into_owned(),
        unit_id.to_string(),
    )
    .expect_err("tampered package must be rejected");
    assert!(err.contains("فشل التحقق"), "got: {err}");
    assert_eq!(last_applied(&state, &unit_identity.to_string()), None);
}

// ─────────────────────────────────────────────────────────────────────────────
// Negative: kind-scoped policy — UNIT may sign ONLY the three data kinds
// (ADR-0046 I2)
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn unit_issuer_rejected_for_products_kind() {
    let (state, anchor_id) = build_wilaya_state();
    let unit_identity = Uuid::new_v4();
    let unit_id = Uuid::new_v4();
    seed_unit(&state, unit_identity, unit_id, CredentialStatus::Active, anchor_id, WILAYA_CODE);
    set_wilaya_admin_session(&state);

    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("products.sync");
    let package = sign_v2_package(
        products_package("prod-unit-1", unit_identity, unit_id, 1),
        UNIT_SECRET,
    );
    write_encrypted(&package, UNIT_SECRET, &path);

    let err = import_products_package_impl(&state, path.to_string_lossy().into_owned())
        .expect_err("UNIT-issued products package must be rejected");
    assert!(err.contains("غير مسموح له بصنف الحزمة"), "got: {err}");
    assert_eq!(last_applied(&state, &unit_identity.to_string()), None);
}

#[test]
fn unit_issuer_rejected_for_identity_access_kind() {
    // identity_access imports are UNIT-side only (SEC-003-06-b) — the authz
    // layer denies them on a WILAYA node before the verifier is reached. The
    // kind-scoped UNIT rejection for `identity_access` is therefore exercised
    // at the verifier level (`unit_issuer_rejected_for_non_data_kind` covers
    // every non-data kind, identity_access included).
    let (state, anchor_id) = build_wilaya_state();
    let unit_identity = Uuid::new_v4();
    let unit_id = Uuid::new_v4();
    seed_unit(&state, unit_identity, unit_id, CredentialStatus::Active, anchor_id, WILAYA_CODE);
    set_wilaya_admin_session(&state);

    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("idacc.sync");
    let package = sign_v2_package(
        identity_access_package("idacc-unit-1", unit_identity, unit_id, 1),
        UNIT_SECRET,
    );
    write_encrypted(&package, UNIT_SECRET, &path);

    // The authz boundary (UNIT-only) rejects the WILAYA call outright; the
    // verifier kind policy is a second, unreachable-by-design defense line.
    let err = import_identity_access_package_impl(&state, path.to_string_lossy().into_owned())
        .expect_err("identity_access on WILAYA is denied by authz");
    assert!(err.contains("غير مصرح"), "got: {err}");
    assert_eq!(last_applied(&state, &unit_identity.to_string()), None);
}

#[test]
fn unit_issuer_rejected_for_trust_kind() {
    let (state, anchor_id) = build_wilaya_state();
    let unit_identity = Uuid::new_v4();
    let unit_id = Uuid::new_v4();
    seed_unit(&state, unit_identity, unit_id, CredentialStatus::Active, anchor_id, WILAYA_CODE);
    set_wilaya_admin_session(&state);

    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("trust.sync");
    let package = sign_v2_package(
        trust_package("trust-unit-1", unit_identity, unit_id, 1),
        UNIT_SECRET,
    );
    write_encrypted(&package, UNIT_SECRET, &path);

    let err = import_trust_package_impl(&state, path.to_string_lossy().into_owned())
        .expect_err("UNIT-issued trust package must be rejected");
    assert!(err.contains("غير مسموح له بصنف الحزمة"), "got: {err}");
    assert_eq!(last_applied(&state, &unit_identity.to_string()), None);
}

#[test]
fn unit_issuer_rejected_for_registry_kind() {
    let (state, anchor_id) = build_wilaya_state();
    let unit_identity = Uuid::new_v4();
    let unit_id = Uuid::new_v4();
    seed_unit(&state, unit_identity, unit_id, CredentialStatus::Active, anchor_id, WILAYA_CODE);
    set_wilaya_admin_session(&state);

    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("registry.sync");
    let package = sign_v2_package(
        registry_package("registry-unit-1", unit_identity, unit_id, 1),
        UNIT_SECRET,
    );
    write_encrypted(&package, UNIT_SECRET, &path);

    let err = import_registry_package_impl(&state, path.to_string_lossy().into_owned())
        .expect_err("UNIT-issued registry package must be rejected");
    assert!(err.contains("غير مسموح له بصنف الحزمة"), "got: {err}");
    assert_eq!(last_applied(&state, &unit_identity.to_string()), None);
}

// ─────────────────────────────────────────────────────────────────────────────
// Negative: transport discipline (B4 ledger unchanged)
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn unit_replay_sequence_is_rejected_and_ledger_not_consumed() {
    let (state, anchor_id) = build_wilaya_state();
    let unit_identity = Uuid::new_v4();
    let unit_id = Uuid::new_v4();
    seed_unit(&state, unit_identity, unit_id, CredentialStatus::Active, anchor_id, WILAYA_CODE);
    set_wilaya_admin_session(&state);
    let product_id = common::create_test_product(&state, "Bread", 20.0, 2026);

    let dir = tempfile::tempdir().expect("temp dir");
    let first = sign_v2_package(
        stock_movements_package(
            "sm-replay-1",
            unit_identity,
            unit_id,
            1,
            vec![movement("m1", &product_id, Some(&unit_id.to_string()), StockMovementType::In)],
        ),
        UNIT_SECRET,
    );
    write_encrypted(&first, UNIT_SECRET, &dir.path().join("first.sync"));
    import_stock_movements_package_impl(
        &state,
        dir.path().join("first.sync").to_string_lossy().into_owned(),
        unit_id.to_string(),
    )
    .expect("sequence 1 accepted");

    let replay = sign_v2_package(
        stock_movements_package(
            "sm-replay-2",
            unit_identity,
            unit_id,
            1,
            vec![movement("m2", &product_id, Some(&unit_id.to_string()), StockMovementType::Out)],
        ),
        UNIT_SECRET,
    );
    write_encrypted(&replay, UNIT_SECRET, &dir.path().join("replay.sync"));
    let err = import_stock_movements_package_impl(
        &state,
        dir.path().join("replay.sync").to_string_lossy().into_owned(),
        unit_id.to_string(),
    )
    .expect_err("replay must be rejected");
    assert!(err.contains("إعادة بث"), "got: {err}");
    assert_eq!(last_applied(&state, &unit_identity.to_string()), Some(1));
}

#[test]
fn unit_out_of_order_sequence_is_rejected() {
    let (state, anchor_id) = build_wilaya_state();
    let unit_identity = Uuid::new_v4();
    let unit_id = Uuid::new_v4();
    seed_unit(&state, unit_identity, unit_id, CredentialStatus::Active, anchor_id, WILAYA_CODE);
    set_wilaya_admin_session(&state);

    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("daily.sync");
    let package = sign_v2_package(
        daily_report_package("dr-oof-1", unit_identity, unit_id, 3),
        UNIT_SECRET,
    );
    write_encrypted(&package, UNIT_SECRET, &path);

    let err = import_daily_report_package_impl(
        &state,
        path.to_string_lossy().into_owned(),
        unit_id.to_string(),
    )
    .expect_err("out-of-order must be rejected");
    assert!(err.contains("انتهاك ترتيب النقل"), "got: {err}");
    assert_eq!(last_applied(&state, &unit_identity.to_string()), None);
}

#[test]
fn unit_issuer_rejected_when_import_unit_id_missing() {
    let (state, anchor_id) = build_wilaya_state();
    let unit_identity = Uuid::new_v4();
    let unit_id = Uuid::new_v4();
    seed_unit(&state, unit_identity, unit_id, CredentialStatus::Active, anchor_id, WILAYA_CODE);
    set_wilaya_admin_session(&state);

    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("mov.sync");
    let package = sign_v2_package(
        stock_movements_package("sm-no-target", unit_identity, unit_id, 1, vec![]),
        UNIT_SECRET,
    );
    write_encrypted(&package, UNIT_SECRET, &path);

    let err = import_stock_movements_package_impl(
        &state,
        path.to_string_lossy().into_owned(),
        "   ".to_string(),
    )
    .expect_err("missing import unit id must be rejected");
    assert!(err.contains("لا تطابق هوية المُصدِر الموثّقة"), "got: {err}");
    assert_eq!(last_applied(&state, &unit_identity.to_string()), None);
}

// ─────────────────────────────────────────────────────────────────────────────
// SYNC-007 (F-01 / F-02): deferred finding closure
// ─────────────────────────────────────────────────────────────────────────────

/// F-01 (SYNC-006 INFO): UNIT issuer → UNIT importer must be rejected through
/// the REAL production pipeline (authz → file loading → decrypt → deserialize
/// → integrity → signature → kind policy), with NO mutation and NO per-issuer
/// ledger advancement. The rejection happens at ADR-0046 I3.3 (`importer_is_wilaya`),
/// before membership/binding — a UNIT node cannot import its own or any UNIT's
/// data packages.
#[test]
fn unit_issuer_rejected_when_importer_is_a_unit_node() {
    let (state, anchor_id, local_unit_id) = build_unit_state();
    // The node's own ACTIVE UNIT certificate (its local identity, same as a
    // provisioned UNIT node) + its local `units` row.
    let unit_identity = Uuid::new_v4();
    seed_unit(
        &state,
        unit_identity,
        local_unit_id,
        CredentialStatus::Active,
        anchor_id,
        WILAYA_CODE,
    );
    set_wilaya_admin_session(&state);
    let product_id = common::create_test_product(&state, "Bread", 20.0, 2026);

    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("mov.sync");
    let package = sign_v2_package(
        stock_movements_package(
            "sm-unit-node",
            unit_identity,
            local_unit_id,
            1,
            vec![movement(
                "m1",
                &product_id,
                Some(&local_unit_id.to_string()),
                StockMovementType::In,
            )],
        ),
        UNIT_SECRET,
    );
    write_encrypted(&package, UNIT_SECRET, &path);

    let err = import_stock_movements_package_impl(
        &state,
        path.to_string_lossy().into_owned(),
        local_unit_id.to_string(),
    )
    .expect_err("UNIT issuer must be rejected on a UNIT importer (ADR-0046 I3.3/I5)");
    assert!(err.contains("تُستورد فقط على عقدة WILAYA"), "got: {err}");
    assert_eq!(
        movement_rows(&state, &local_unit_id.to_string()).len(),
        0,
        "rejected UNIT→UNIT import must not mutate stock_movements"
    );
    assert_eq!(
        last_applied(&state, &unit_identity.to_string()),
        None,
        "rejected UNIT→UNIT import must not advance the per-issuer ledger"
    );
}

/// F-02 (SYNC-006 INFO): an empty payload `unit_id` is NOT an independent
/// identity — per ADR-0046 I3.7d it is treated like an absent id and the
/// movement is restamped to the authenticated `cert.subject_id`. The empty
/// string must never grant an alternative identity (the foreign-unit REJECT
/// case is covered by `unit_issuer_rejected_when_payload_movement_belongs_to_another_unit`).
#[test]
fn empty_payload_unit_id_is_restamped_to_authenticated_subject() {
    let (state, anchor_id) = build_wilaya_state();
    let unit_identity = Uuid::new_v4();
    let unit_id = Uuid::new_v4();
    seed_unit(&state, unit_identity, unit_id, CredentialStatus::Active, anchor_id, WILAYA_CODE);
    set_wilaya_admin_session(&state);
    let product_id = common::create_test_product(&state, "Bread", 20.0, 2026);

    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("mov.sync");
    let package = sign_v2_package(
        stock_movements_package(
            "sm-empty-unit",
            unit_identity,
            unit_id,
            1,
            vec![movement("m1", &product_id, Some(""), StockMovementType::In)],
        ),
        UNIT_SECRET,
    );
    write_encrypted(&package, UNIT_SECRET, &path);

    let result = import_stock_movements_package_impl(
        &state,
        path.to_string_lossy().into_owned(),
        unit_id.to_string(),
    )
    .expect("empty payload unit_id is not an independent identity (ADR-0046 I3.7d)");
    assert_eq!(result.movement_count, 1);
    assert_eq!(last_applied(&state, &unit_identity.to_string()), Some(1));

    let rows = movement_rows(&state, &unit_id.to_string());
    assert_eq!(rows.len(), 1, "movement persisted under the authenticated unit");
    for (_, stored_unit, _) in rows {
        assert_eq!(
            stored_unit,
            unit_id.to_string(),
            "empty payload unit_id must be restamped to the authenticated subject"
        );
    }
}