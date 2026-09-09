//! UNIT local read-only ContractCatalog entitlement projection (Phase 4).
//!
//! Exercises the application-service surface and the authorization boundary
//! that `commands::list_unit_contract_entitlements` delegates to:
//!   - the UNIT caller identity is derived server-side (`get_current_unit_id`),
//!     so no frontend-supplied `unit_id` is accepted;
//!   - the projection is scoped to the caller's own local unit rows;
//!   - `Action::ReadUnitEntitlements` is UNIT-node scoped, read-only, and
//!     never exposes the WILAYA `ReadContractProjection` authority.
//!
//! CANCELLED / historical rows remain visible in the lifecycle projection.

#[allow(dead_code)]
mod common;

use grpc_lib::application::authz::{Action, AuthorizationError};
use grpc_lib::application::services::{ContractService, SettingsService};
use grpc_lib::commands::{authorize_command, AppState};
use grpc_lib::db::{ConnectionFactory, Database};
use grpc_lib::errors::{AppError, AuthenticationError};
use grpc_lib::models::{NodeType, UserRole, WilayaNodeConfiguration};
use grpc_lib::repositories::SettingsRepository;
use rusqlite::params;

const NOW: &str = "2026-01-01T00:00:00Z";

/// Build a UNIT node whose local unit identity is `unit_id` (a real `units.id`
/// resolved from settings.unit_name + wilaya_code), with nothing seeded yet.
fn unit_db(unit_id: &str) -> Database {
    let db = ConnectionFactory::new_for_test().expect("db");
    SettingsRepository::new(db.executor())
        .update_unit_node_settings(unit_id, "16")
        .expect("set unit settings");
    db.get_connection()
        .execute(
            "UPDATE settings SET configured = 1, current_year = 2026 WHERE id = 1",
            [],
        )
        .expect("configured");
    db.get_connection()
        .execute(
            "INSERT INTO units (id, code, name, wilaya_code, created_at) VALUES (?1, ?2, ?3, '16', ?4)",
            params![unit_id, unit_id, unit_id, NOW],
        )
        .expect("seed local unit");
    db
}

fn seed_product(db: &Database, id: &str, name: &str, year: i32) {
    db.get_connection()
        .execute(
            "INSERT INTO products (id, name, base_price, year, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, name, 10000.0f64, year, NOW],
        )
        .expect("seed product");
}

fn seed_supplier(db: &Database, id: &str, name: &str) {
    db.get_connection()
        .execute(
            "INSERT INTO suppliers (id, name, contact_info, active, created_at) VALUES (?1, ?2, NULL, 1, ?3)",
            params![id, name, NOW],
        )
        .expect("seed supplier");
}

#[allow(clippy::too_many_arguments)]
fn seed_contract(
    db: &Database,
    contract_id: &str,
    unit_id: &str,
    supplier_id: &str,
    fiscal_year: i32,
    status: &str,
    created_at: &str,
) -> String {
    db.get_connection()
        .execute(
            "INSERT INTO contracts (id, contract_reference, unit_id, supplier_id, fiscal_year, status, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                contract_id,
                format!("REF-{contract_id}"),
                unit_id,
                supplier_id,
                fiscal_year,
                status,
                created_at,
            ],
        )
        .expect("seed contract");
    contract_id.to_string()
}

fn seed_contract_product(
    db: &Database,
    cp_id: &str,
    contract_id: &str,
    product_id: &str,
    agreed_price_ht: Option<f64>,
) -> String {
    // Seed the authoritative snapshot: agreed HT + computed price_ttc.
    let scaled = agreed_price_ht.map(|p| p * 100.0);
    db.get_connection()
        .execute(
            "INSERT INTO contract_products (id, contract_id, product_id, proposed_price_ht, agreed_price_ht, price_ttc, created_at)
             VALUES (?1, ?2, ?3, 12000.0, ?4, ?5, ?6)",
            params![cp_id, contract_id, product_id, scaled, scaled, NOW],
        )
        .expect("seed contract_product");
    cp_id.to_string()
}

/// Direct SQL seed of a `contract_allocations` row.
#[allow(clippy::too_many_arguments)]
fn seed_allocation(
    db: &Database,
    id: &str,
    contract_id: &str,
    cp_id: &str,
    unit_id: &str,
    product_id: &str,
    fiscal_year: i32,
    contracted: f64,
    fulfilled: f64,
    released: f64,
    reserved: f64,
    state: &str,
    created_at: &str,
) {
    db.get_connection()
        .execute(
            "INSERT INTO contract_allocations (
                id, contract_id, contract_product_id, unit_id, product_id,
                fiscal_year, contracted_quantity, fulfilled_quantity,
                released_quantity, reserved_quantity, entitlement_state, version, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 1, ?12)",
            params![
                id,
                contract_id,
                cp_id,
                unit_id,
                product_id,
                fiscal_year,
                contracted * 1000.0,
                fulfilled * 1000.0,
                released * 1000.0,
                reserved * 1000.0,
                state,
                created_at,
            ],
        )
        .expect("seed allocation");
}

fn entitlements_for(
    db: &Database,
    unit_id: &str,
) -> Vec<grpc_lib::models::UnitContractEntitlement> {
    ContractService::new(db.executor())
        .list_unit_entitlements(unit_id)
        .expect("list unit entitlements")
}

fn resolved_unit_id(db: &Database) -> String {
    SettingsService::new(db.executor())
        .get_current_unit_id()
        .expect("resolve current unit id")
        .expect("unit must be present")
}

// ─────────────────────────────────────────────────────────────────────────────
// Authz boundary (mirrors what `authorize_command` enforces for the command)
// ─────────────────────────────────────────────────────────────────────────────

fn configured_state(db: Database) -> AppState {
    AppState::new_for_test(db)
}

fn set_session(state: &AppState, role: &str) {
    let mut session = common::create_test_session("u1", "bob", role);
    session.user_role = UserRole::from(role.to_string());
    common::insert_test_user(state, "u1", "bob", role);
    *state.current_session.lock().expect("session mutex") = Some(session);
}

#[test]
fn unit_node_is_authorized_for_read_unit_entitlements() {
    let db = unit_db("unit-a");
    let state = configured_state(db);
    set_session(&state, "Admin");

    let res = authorize_command(&state, Action::ReadUnitEntitlements, None);
    assert!(
        res.is_ok(),
        "UNIT admin must be able to read own entitlements: {res:?}"
    );
}

#[test]
fn unit_user_role_is_authorized_for_read_unit_entitlements() {
    let db = unit_db("unit-a");
    let state = configured_state(db);
    set_session(&state, "User");

    let res = authorize_command(&state, Action::ReadUnitEntitlements, None);
    assert!(
        res.is_ok(),
        "UNIT non-admin must be able to read own entitlements: {res:?}"
    );
}

#[test]
fn wilaya_node_cannot_use_read_unit_entitlements() {
    let db = ConnectionFactory::new_for_test().expect("db");
    SettingsRepository::new(db.executor())
        .configure_wilaya(&WilayaNodeConfiguration {
            node_type: NodeType::Wilaya,
            wilaya_code: Some("16".into()),
            wilaya_name: Some("Alger".into()),
        })
        .expect("configure wilaya");
    let state = configured_state(db);
    set_session(&state, "Admin");

    let err = authorize_command(&state, Action::ReadUnitEntitlements, None).expect_err("deny");
    assert!(
        matches!(
            err,
            AppError::Authorization(AuthorizationError::RequiresUnitNode)
        ),
        "WILAYA node must be denied ReadUnitEntitlements: {err:?}"
    );
}

#[test]
fn unauthenticated_caller_is_denied_read_unit_entitlements() {
    let db = unit_db("unit-a");
    // Note: no session is set.
    let state = configured_state(db);

    let err = authorize_command(&state, Action::ReadUnitEntitlements, None).expect_err("deny");
    assert!(
        matches!(
            err,
            AppError::Authentication(AuthenticationError::SessionNotFound)
        ),
        "unauthenticated caller must be denied: {err:?}"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Server-side unit scoping (no frontend-supplied unit_id)
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn current_unit_id_is_resolved_from_node_settings_not_client_input() {
    let db = unit_db("unit-local");
    // The canonical id resolves from settings.unit_name + wilaya_code.
    assert_eq!(resolved_unit_id(&db), "unit-local");
}

#[test]
fn unit_sees_only_its_own_rows_no_cross_unit_leakage() {
    let unit = "unit-a";
    let other = "unit-b";
    let db = unit_db(unit);
    db.get_connection()
        .execute(
            "INSERT INTO units (id, code, name, wilaya_code, created_at) VALUES (?1, ?2, ?3, '16', ?4)",
            params![other, other, other, NOW],
        )
        .expect("seed other unit");

    seed_product(&db, "prod-1", "Rice", 2026);
    seed_supplier(&db, "s-1", "Fournisseur A");
    seed_supplier(&db, "s-2", "Fournisseur B");

    // Unit's own allocation.
    let c1 = seed_contract(&db, "ctr-a", unit, "s-1", 2026, "active", NOW);
    let cp1 = seed_contract_product(&db, "cp-a", &c1, "prod-1", Some(110.0));
    seed_allocation(
        &db, "alloc-a", &c1, &cp1, unit, "prod-1", 2026, 100.0, 0.0, 0.0, 0.0, "ACTIVE", NOW,
    );

    // Another unit's allocation (must NEVER appear in unit-a's projection).
    let c2 = seed_contract(&db, "ctr-b", other, "s-2", 2026, "active", NOW);
    let cp2 = seed_contract_product(&db, "cp-b", &c2, "prod-1", Some(95.0));
    seed_allocation(
        &db, "alloc-b", &c2, &cp2, other, "prod-1", 2026, 80.0, 0.0, 0.0, 0.0, "ACTIVE", NOW,
    );

    let ents = entitlements_for(&db, unit);
    assert_eq!(ents.len(), 1, "unit-a must see exactly its own row");
    assert_eq!(ents[0].product_name, "Rice");
    assert_eq!(ents[0].supplier_name, "Fournisseur A");
    assert!(
        ents.iter()
            .all(|e| e.product_id == "prod-1" && e.price_ttc == Some(110.0)),
        "only unit-a's row with its agreed price (price_ttc snapshot)"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Projection content & lifecycle semantics
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn effective_remaining_is_contract_minus_fulfilled_minus_released_minus_reserved() {
    let db = unit_db("unit-a");
    seed_product(&db, "prod-1", "Rice", 2026);
    seed_supplier(&db, "s-1", "Fournisseur A");
    let c = seed_contract(&db, "ctr-1", "unit-a", "s-1", 2026, "active", NOW);
    let cp = seed_contract_product(&db, "cp-1", &c, "prod-1", Some(110.0));
    seed_allocation(
        &db, "alloc-1", &c, &cp, "unit-a", "prod-1", 2026, 100.0, 10.0, 20.0, 5.0, "ACTIVE", NOW,
    );

    let ents = entitlements_for(&db, "unit-a");
    assert_eq!(ents.len(), 1);
    // effective_remaining = 100 - 10 - 20 - 5 = 65
    assert!((ents[0].effective_remaining - 65.0).abs() < f64::EPSILON);
    assert_eq!(ents[0].contracted_quantity, 100.0);
    assert_eq!(ents[0].fulfilled_quantity, 10.0);
    assert_eq!(ents[0].released_quantity, 20.0);
    assert_eq!(ents[0].reserved_quantity, 5.0);
}

#[test]
fn reserved_quantity_is_present_when_non_zero() {
    let db = unit_db("unit-a");
    seed_product(&db, "prod-1", "Rice", 2026);
    seed_supplier(&db, "s-1", "Fournisseur A");
    let c = seed_contract(&db, "ctr-1", "unit-a", "s-1", 2026, "active", NOW);
    let cp = seed_contract_product(&db, "cp-1", &c, "prod-1", Some(110.0));
    seed_allocation(
        &db, "alloc-1", &c, &cp, "unit-a", "prod-1", 2026, 100.0, 0.0, 0.0, 40.0, "ACTIVE", NOW,
    );

    let ents = entitlements_for(&db, "unit-a");
    assert_eq!(ents[0].reserved_quantity, 40.0);
    assert!((ents[0].effective_remaining - 60.0).abs() < f64::EPSILON);
}

#[test]
fn fulfilled_obligation_yields_zero_effective_remaining() {
    let db = unit_db("unit-a");
    seed_product(&db, "prod-1", "Rice", 2026);
    seed_supplier(&db, "s-1", "Fournisseur A");
    let c = seed_contract(&db, "ctr-1", "unit-a", "s-1", 2026, "active", NOW);
    let cp = seed_contract_product(&db, "cp-1", &c, "prod-1", Some(110.0));
    seed_allocation(
        &db, "alloc-1", &c, &cp, "unit-a", "prod-1", 2026, 100.0, 100.0, 0.0, 0.0, "ACTIVE", NOW,
    );

    let ents = entitlements_for(&db, "unit-a");
    assert!((ents[0].effective_remaining - 0.0).abs() < f64::EPSILON);
}

#[test]
fn released_quantity_reduces_effective_remaining() {
    let db = unit_db("unit-a");
    seed_product(&db, "prod-1", "Rice", 2026);
    seed_supplier(&db, "s-1", "Fournisseur A");
    let c = seed_contract(&db, "ctr-1", "unit-a", "s-1", 2026, "active", NOW);
    let cp = seed_contract_product(&db, "cp-1", &c, "prod-1", Some(110.0));
    // WILAYA released 200 of a 200 outstanding obligation => effective_remaining == 0
    seed_allocation(
        &db, "alloc-1", &c, &cp, "unit-a", "prod-1", 2026, 200.0, 0.0, 200.0, 0.0, "ACTIVE", NOW,
    );

    let ents = entitlements_for(&db, "unit-a");
    assert_eq!(ents[0].released_quantity, 200.0);
    assert!((ents[0].effective_remaining - 0.0).abs() < f64::EPSILON);
}

#[test]
fn empty_projection_returns_no_rows() {
    let db = unit_db("unit-a");
    assert!(entitlements_for(&db, "unit-a").is_empty());
}

#[test]
fn multiple_fiscal_years_remain_separate_rows() {
    let db = unit_db("unit-a");
    seed_product(&db, "prod-1", "Rice", 2026);
    seed_supplier(&db, "s-x", "Fournisseur X");
    seed_supplier(&db, "s-y", "Fournisseur Y");

    // 2026: Supplier X outstanding obligation.
    let c26 = seed_contract(
        &db,
        "ctr-2026",
        "unit-a",
        "s-x",
        2026,
        "active",
        "2026-01-01T00:00:00Z",
    );
    let cp26 = seed_contract_product(&db, "cp-2026", &c26, "prod-1", Some(100.0));
    seed_allocation(
        &db,
        "alloc-2026",
        &c26,
        &cp26,
        "unit-a",
        "prod-1",
        2026,
        1000.0,
        800.0,
        0.0,
        0.0,
        "ACTIVE",
        "2026-01-01T00:00:00Z",
    );

    // 2027: Supplier Y new/current entitlement.
    let c27 = seed_contract(
        &db,
        "ctr-2027",
        "unit-a",
        "s-y",
        2027,
        "active",
        "2027-01-01T00:00:00Z",
    );
    let cp27 = seed_contract_product(&db, "cp-2027", &c27, "prod-1", Some(95.0));
    seed_allocation(
        &db,
        "alloc-2027",
        &c27,
        &cp27,
        "unit-a",
        "prod-1",
        2027,
        500.0,
        0.0,
        0.0,
        0.0,
        "ACTIVE",
        "2027-01-01T00:00:00Z",
    );

    let ents = entitlements_for(&db, "unit-a");
    assert_eq!(ents.len(), 2, "X and Y must remain TWO separate rows");
    let x = ents
        .iter()
        .find(|e| e.fiscal_year == 2026)
        .expect("2026 row");
    let y = ents
        .iter()
        .find(|e| e.fiscal_year == 2027)
        .expect("2027 row");

    assert_eq!(x.supplier_id, "s-x");
    assert_eq!(x.supplier_name, "Fournisseur X");
    assert!((x.effective_remaining - 200.0).abs() < f64::EPSILON);

    assert_eq!(y.supplier_id, "s-y");
    assert_eq!(y.supplier_name, "Fournisseur Y");
    assert!((y.effective_remaining - 500.0).abs() < f64::EPSILON);
}

#[test]
fn lifecycle_states_remain_visible_including_cancelled() {
    let db = unit_db("unit-a");
    seed_supplier(&db, "s-1", "Fournisseur X");
    seed_supplier(&db, "s-2", "Fournisseur Y");
    seed_supplier(&db, "s-3", "Fournisseur Z");

    // Distinct (product, fiscal_year) per state to satisfy the live-contract
    // index `(unit_id, fiscal_year)` and the single-ACTIVE allocation index.
    let mk = |pid: &str, st: &str, sup: &str, ref_: &str, year: i32| {
        seed_product(&db, pid, pid, year);
        let c = seed_contract(
            &db,
            &format!("ctr-{ref_}"),
            "unit-a",
            sup,
            year,
            "active",
            NOW,
        );
        let cp = seed_contract_product(&db, &format!("cp-{ref_}"), &c, pid, Some(110.0));
        seed_allocation(
            &db,
            &format!("alloc-{ref_}"),
            &c,
            &cp,
            "unit-a",
            pid,
            year,
            100.0,
            0.0,
            0.0,
            0.0,
            st,
            NOW,
        );
    };
    mk("prod-a", "ACTIVE", "s-1", "a", 2026);
    mk("prod-b", "ENDED", "s-2", "b", 2025);
    mk("prod-c", "CANCELLED", "s-3", "c", 2024);

    let ents = entitlements_for(&db, "unit-a");
    assert_eq!(ents.len(), 3);
    let states: Vec<&str> = ents.iter().map(|e| e.entitlement_state.as_str()).collect();
    assert!(states.contains(&"ACTIVE"));
    assert!(states.contains(&"ENDED"));
    assert!(
        states.contains(&"CANCELLED"),
        "CANCELLED rows must stay visible: {states:?}"
    );
}

#[test]
fn stale_local_state_unchanged_until_import() {
    // The projection reflects the LAST imported ContractCatalog state. Here we
    // prove the projection reads the local persisted rows verbatim: changing the
    // local released_quantity (as a fresh import would) is reflected, whereas no
    // real-time WILAYA lookup is performed.
    let db = unit_db("unit-a");
    seed_product(&db, "prod-1", "Rice", 2026);
    seed_supplier(&db, "s-1", "Fournisseur X");
    let c = seed_contract(&db, "ctr-1", "unit-a", "s-1", 2026, "active", NOW);
    let cp = seed_contract_product(&db, "cp-1", &c, "prod-1", Some(100.0));
    seed_allocation(
        &db, "alloc-1", &c, &cp, "unit-a", "prod-1", 2026, 200.0, 0.0, 0.0, 0.0, "ACTIVE", NOW,
    );

    assert!((entitlements_for(&db, "unit-a")[0].effective_remaining - 200.0).abs() < f64::EPSILON);

    // Simulate the next catalog import: WILAYA releases the 200 outstanding.
    db.get_connection()
        .execute(
            "UPDATE contract_allocations SET released_quantity = 200000 WHERE id = 'alloc-1'",
            [],
        )
        .expect("update local released");
    let ents = entitlements_for(&db, "unit-a");
    assert_eq!(ents[0].released_quantity, 200.0);
    assert!((ents[0].effective_remaining - 0.0).abs() < f64::EPSILON);
}
