//! Integration tests: ADR-0064 UNIT-owned data deletion.
//!
//! Covers the fail-closed preflight (§4.6–§4.10, §5.1, §5.5), the exact ordered
//! purge (§4.4), Class-B/global preservation (§4.2), atomicity/rollback (§4.5),
//! and the audit-transaction contract.
//!
//! Every test asserts database state before **and** after the operation — not
//! merely that an error was returned. Preflight rejections prove that no
//! eligible row was deleted; the injected mid-purge failure proves full
//! rollback.

use grpc_lib::application::services::{AuditTxService, UnitService, UserContext};
use grpc_lib::db::{ConnectionFactory, Database};
use grpc_lib::domain::audit::AuditAction;
use grpc_lib::errors::{AppError, BusinessLogicError};
use grpc_lib::infrastructure::security::Argon2PasswordHashProvider;
use grpc_lib::models::CreateUnitRequest;
use rusqlite::{params, Connection};
use tempfile::TempDir;

const NOW: &str = "2025-01-01T00:00:00Z";

// ─────────────────────────────────────────────────────────────────────────────
// Harness
// ─────────────────────────────────────────────────────────────────────────────

struct Harness {
    _dir: TempDir,
    db: Database,
    admin_id: String,
}

impl Harness {
    fn new(name: &str) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(format!("{name}.db"));
        let db = ConnectionFactory::new_with_path(&path).expect("db");
        let admin_id = db
            .get_connection()
            .query_row(
                "SELECT id FROM users WHERE username = 'admin' LIMIT 1",
                [],
                |r| r.get::<_, String>(0),
            )
            .expect("seeded admin");
        Self {
            _dir: dir,
            db,
            admin_id,
        }
    }

    fn conn(&self) -> &Connection {
        self.db.get_connection()
    }

    /// Create a UNIT with its canonical `user` operator (real service path).
    fn create_unit(&mut self, code: &str, name: &str) -> (String, String) {
        let admin = self.admin_id.clone();
        let ctx = UserContext::new(&admin, "admin", None);
        let (unit, user_id) =
            AuditTxService::execute_with_audit(&mut self.db, AuditAction::CreateUnit, &ctx, |tx| {
                UnitService::new(tx.executor, &Argon2PasswordHashProvider).create_unit(
                    &CreateUnitRequest {
                        code: code.to_string(),
                        name: name.to_string(),
                    },
                    "WILAYA-1",
                )
            })
            .expect("create unit");
        (unit.id, user_id)
    }

    /// Delete through the real IPC entry point (`execute_with_audit` + service).
    fn delete(&mut self, unit_id: &str) -> Result<(), AppError> {
        let admin = self.admin_id.clone();
        let ctx = UserContext::new(&admin, "admin", None);
        AuditTxService::execute_with_audit(&mut self.db, AuditAction::DeleteUnit, &ctx, |tx| {
            UnitService::new(tx.executor, &Argon2PasswordHashProvider).delete_unit(unit_id)
        })
    }

    fn scalar(&self, sql: &str) -> i64 {
        self.conn().query_row(sql, [], |r| r.get(0)).unwrap()
    }

    fn scalar1(&self, sql: &str, p1: &str) -> i64 {
        self.conn().query_row(sql, [p1], |r| r.get(0)).unwrap()
    }

    fn exists(&self, sql: &str) -> bool {
        self.scalar(sql) > 0
    }
}

fn is_delete_forbidden(err: &AppError) -> bool {
    matches!(
        err,
        AppError::BusinessLogic(BusinessLogicError::DeleteForbidden { .. })
    )
}

/// Extract the rejection reason from a `DeleteForbidden` error (panics otherwise).
fn delete_forbidden_reason(err: &AppError) -> String {
    match err {
        AppError::BusinessLogic(BusinessLogicError::DeleteForbidden { reason, .. }) => {
            reason.clone()
        }
        other => panic!("expected DeleteForbidden, got {other:?}"),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Seed helpers
// ─────────────────────────────────────────────────────────────────────────────

fn seed_product(conn: &Connection, id: &str) {
    conn.execute(
        "INSERT INTO products (id, name, base_price, year, created_at, purchase_unit, \
         consumption_unit, conversion_factor, tva_classification) \
         VALUES (?1, ?2, 100, 2025, ?3, 1, 1, 1, 0)",
        params![id, id, NOW],
    )
    .unwrap();
}

fn seed_supplier(conn: &Connection, id: &str) {
    conn.execute(
        "INSERT INTO suppliers (id, name, created_at) VALUES (?1, ?2, ?3)",
        params![id, id, NOW],
    )
    .unwrap();
}

fn seed_contract(conn: &Connection, id: &str, unit_id: &str, supplier_id: &str, fy: i64) {
    conn.execute(
        "INSERT INTO contracts (id, contract_reference, unit_id, supplier_id, fiscal_year, created_at) \
         VALUES (?1, ?1, ?2, ?3, ?4, ?5)",
        params![id, unit_id, supplier_id, fy, NOW],
    )
    .unwrap();
}

fn seed_contract_product(conn: &Connection, id: &str, contract_id: &str, product_id: &str) {
    conn.execute(
        "INSERT INTO contract_products (id, contract_id, product_id, proposed_price_ht, created_at) \
         VALUES (?1, ?2, ?3, 100, ?4)",
        params![id, contract_id, product_id, NOW],
    )
    .unwrap();
}

fn seed_allocation(
    conn: &Connection,
    id: &str,
    contract_id: &str,
    contract_product_id: &str,
    unit_id: &str,
    product_id: &str,
    fy: i64,
) {
    conn.execute(
        "INSERT INTO contract_allocations (id, contract_id, contract_product_id, unit_id, \
         product_id, fiscal_year, contracted_quantity, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 100, ?7)",
        params![
            id,
            contract_id,
            contract_product_id,
            unit_id,
            product_id,
            fy,
            NOW
        ],
    )
    .unwrap();
}

fn seed_allocation_exception(conn: &Connection, id: &str, allocation_id: &str) {
    conn.execute(
        "INSERT INTO contract_allocation_exceptions (id, allocation_id, released_quantity, \
         reason_code, created_by, created_at) \
         VALUES (?1, ?2, 1, 'OTHER_AUTHORIZED', 'admin', ?3)",
        params![id, allocation_id, NOW],
    )
    .unwrap();
}

fn seed_order(conn: &Connection, id: &str, supplier_id: &str, unit_id: Option<&str>, fy: i64) {
    conn.execute(
        "INSERT INTO supplier_orders (id, order_date, supplier_id, supplier_name, created_at, \
         unit_id, fiscal_year) VALUES (?1, '2025-01-01', ?2, ?2, ?3, ?4, ?5)",
        params![id, supplier_id, NOW, unit_id, fy],
    )
    .unwrap();
}

fn seed_order_item(
    conn: &Connection,
    id: &str,
    order_id: &str,
    product_id: &str,
    unit_id: Option<&str>,
    fy: i64,
) {
    conn.execute(
        "INSERT INTO supplier_order_items (id, order_id, product_id, quantity, unit_price, \
         total_cost, unit_id, fiscal_year) VALUES (?1, ?2, ?3, 1, 10, 10, ?4, ?5)",
        params![id, order_id, product_id, unit_id, fy],
    )
    .unwrap();
}

fn seed_soia(conn: &Connection, id: &str, item_id: &str, allocation_id: &str) {
    conn.execute(
        "INSERT INTO supplier_order_item_allocations (id, item_id, allocation_id, quantity, \
         unit_price, total_cost, created_at) VALUES (?1, ?2, ?3, 1, 10, 10, ?4)",
        params![id, item_id, allocation_id, NOW],
    )
    .unwrap();
}

fn seed_unit_supplier(conn: &Connection, unit_id: &str, supplier_id: &str) {
    conn.execute(
        "INSERT INTO unit_suppliers (unit_id, supplier_id, created_at) VALUES (?1, ?2, ?3)",
        params![unit_id, supplier_id, NOW],
    )
    .unwrap();
}

fn seed_layer(conn: &Connection, id: &str, unit_id: &str, product_id: &str, fy: i64) {
    conn.execute(
        "INSERT INTO fifo_stock_layers (id, unit_id, product_id, source_type, unit_cost, \
         qty_original, qty_remaining, received_at, created_by, origin_fiscal_year) \
         VALUES (?1, ?2, ?3, 'ORDER', 10, 1, 1, ?4, 'tester', ?5)",
        params![id, unit_id, product_id, NOW, fy],
    )
    .unwrap();
}

fn seed_movement(
    conn: &Connection,
    id: &str,
    product_id: &str,
    movement_type: &str,
    unit_id: Option<&str>,
    fy: i64,
) {
    conn.execute(
        "INSERT INTO stock_movements (id, product_id, movement_type, quantity, balance_before, \
         balance_after, timestamp, user_id, username, unit_id, fiscal_year) \
         VALUES (?1, ?2, ?3, 1, 0, 1, ?4, 'u1', 'user', ?5, ?6)",
        params![id, product_id, movement_type, NOW, unit_id, fy],
    )
    .unwrap();
}

fn seed_ilm(conn: &Connection, id: &str, unit_id: &str, movement_id: &str, layer_id: &str) {
    conn.execute(
        "INSERT INTO inventory_layer_consumptions (id, unit_id, movement_id, layer_id, quantity, \
         unit_cost, total_cost, consumed_at) VALUES (?1, ?2, ?3, ?4, 1, 10, 10, ?5)",
        params![id, unit_id, movement_id, layer_id, NOW],
    )
    .unwrap();
}

fn seed_daily_report(conn: &Connection, id: &str, unit_id: Option<&str>, fy: i64) {
    conn.execute(
        "INSERT INTO daily_reports (id, date, unit_id, created_at, fiscal_year) \
         VALUES (?1, '2025-01-01', ?2, ?3, ?4)",
        params![id, unit_id, NOW, fy],
    )
    .unwrap();
}

fn seed_meal(conn: &Connection, id: &str, report_id: &str) {
    conn.execute(
        "INSERT INTO daily_report_meals (id, daily_report_id, meal_type) VALUES (?1, ?2, 'lunch')",
        params![id, report_id],
    )
    .unwrap();
}

fn seed_meal_item(
    conn: &Connection,
    id: &str,
    meal_id: &str,
    product_id: &str,
    fifo_layer_id: Option<&str>,
) {
    conn.execute(
        "INSERT INTO daily_report_meal_items (id, meal_id, product_id, quantity, unit_price, \
         total_cost, fifo_layer_id) VALUES (?1, ?2, ?3, 1, 10, 10, ?4)",
        params![id, meal_id, product_id, fifo_layer_id],
    )
    .unwrap();
}

fn seed_monthly_report(conn: &Connection, id: &str, unit_id: &str) {
    conn.execute(
        "INSERT INTO monthly_reports (id, unit_id, report_year, report_month, imported_at, imported_by) \
         VALUES (?1, ?2, 2025, 1, ?3, 'tester')",
        params![id, unit_id, NOW],
    )
    .unwrap();
}

fn seed_monthly_snapshot(conn: &Connection, id: &str, unit_id: &str, product_id: &str) {
    conn.execute(
        "INSERT INTO unit_monthly_snapshots (id, unit_id, unit_name, report_year, report_month, \
         product_id, product_name, computed_at) VALUES (?1, ?2, 'U', 2025, 1, ?3, 'P', ?4)",
        params![id, unit_id, product_id, NOW],
    )
    .unwrap();
}

fn seed_unit_identity(conn: &Connection, id: &str, unit_id: &str) {
    conn.execute(
        "INSERT INTO identity_store (identity_id, subject_type, subject_id, credential_id, \
         generation, status, public_key, algorithm_version, created_at, updated_at) \
         VALUES (?1, 'UNIT', ?2, 'cred', 1, 'ACTIVE', X'00', 1, ?3, ?3)",
        params![id, unit_id, NOW],
    )
    .unwrap();
}

fn seed_identity(conn: &Connection, id: &str, subject_type: &str, subject_id: &str) {
    conn.execute(
        "INSERT INTO identity_store (identity_id, subject_type, subject_id, credential_id, \
         generation, status, public_key, algorithm_version, created_at, updated_at) \
         VALUES (?1, ?2, ?3, 'cred', 1, 'ACTIVE', X'00', 1, ?4, ?4)",
        params![id, subject_type, subject_id, NOW],
    )
    .unwrap();
}

fn seed_inventory_stock(conn: &Connection, id: &str, product_id: &str) {
    conn.execute(
        "INSERT INTO inventory_stocks (id, product_id, consumption_unit, last_updated) \
         VALUES (?1, ?2, 1, ?3)",
        params![id, product_id, NOW],
    )
    .unwrap();
}

fn seed_opening_balance(conn: &Connection, id: &str, product_id: &str, fy: i64) {
    conn.execute(
        "INSERT INTO opening_balance_snapshots (id, product_id, fiscal_year, opening_quantity, \
         created_at, created_by) VALUES (?1, ?2, ?3, 10, ?4, 'tester')",
        params![id, product_id, fy, NOW],
    )
    .unwrap();
}

fn seed_fiscal_export(conn: &Connection, hash: &str, fy: i64) {
    conn.execute(
        "INSERT INTO fiscal_export_snapshots (export_hash, generated_at, generated_by, fiscal_year, \
         movement_count, report_count, inventory_total_value) VALUES (?1, ?2, 'admin', ?3, 5, 3, 1000)",
        params![hash, NOW, fy],
    )
    .unwrap();
}

fn seed_fiscal_year_status(conn: &Connection, year: i64, status: &str, archived: i64) {
    conn.execute(
        "INSERT INTO fiscal_year_status (year, status, archived) VALUES (?1, ?2, ?3)",
        params![year, status, archived],
    )
    .unwrap();
}

fn seed_domain_event(conn: &Connection, tx: &str) {
    conn.execute(
        "INSERT INTO domain_events (transaction_id, sequence_number, category, event_type, event_body) \
         VALUES (?1, 1, 'test', 'TestEvent', '{}')",
        params![tx],
    )
    .unwrap();
}

fn seed_audit_for_user(conn: &Connection, id: &str, user_id: &str) {
    conn.execute(
        "INSERT INTO audit_log (id, user_id, username, action, entity_type, timestamp) \
         VALUES (?1, ?2, 'user', 'Login', 'User', ?3)",
        params![id, user_id, NOW],
    )
    .unwrap();
}

// ─────────────────────────────────────────────────────────────────────────────
// 1 / 19 — Successful full purge of every Class-A entity + Class-B preservation
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn success_purges_all_class_a_and_preserves_class_b() {
    let mut h = Harness::new("del_success");
    let (unit_id, operator_id) = h.create_unit("T100", "Unit 100");

    {
        let c = h.conn();
        seed_product(c, "p1");
        seed_supplier(c, "s1");
        // Contract subtree
        seed_contract(c, "ct1", &unit_id, "s1", 2025);
        seed_contract_product(c, "cp1", "ct1", "p1");
        seed_allocation(c, "al1", "ct1", "cp1", &unit_id, "p1", 2025);
        seed_allocation_exception(c, "ae1", "al1");
        // Order subtree + two-sided allocation join
        seed_order(c, "o1", "s1", Some(&unit_id), 2025);
        seed_order_item(c, "oi1", "o1", "p1", Some(&unit_id), 2025);
        seed_soia(c, "soia1", "oi1", "al1");
        seed_unit_supplier(c, &unit_id, "s1");
        // FIFO + movement + consumption
        seed_layer(c, "l1", &unit_id, "p1", 2025);
        seed_movement(c, "mv1", "p1", "OUT", Some(&unit_id), 2025);
        seed_ilm(c, "ilm1", &unit_id, "mv1", "l1");
        // Report hierarchy
        seed_daily_report(c, "dr1", Some(&unit_id), 2025);
        seed_meal(c, "m1", "dr1");
        seed_meal_item(c, "mi1", "m1", "p1", Some("l1"));
        seed_monthly_report(c, "mr1", &unit_id);
        seed_monthly_snapshot(c, "ms1", &unit_id, "p1");
        seed_unit_identity(c, "id1", &unit_id);
        // Class-B / global data that must survive
        seed_inventory_stock(c, "is1", "p1");
        seed_opening_balance(c, "ob1", "p1", 2025);
        seed_fiscal_export(c, "hash-success", 2025);
        seed_fiscal_year_status(c, 2025, "closed", 1);
        seed_domain_event(c, "tx-domain-1");
    }

    // Baseline Class-B
    let baseline_stocks = h.scalar("SELECT COUNT(*) FROM inventory_stocks");
    let baseline_opening = h.scalar("SELECT COUNT(*) FROM opening_balance_snapshots");
    let baseline_export = h.scalar("SELECT COUNT(*) FROM fiscal_export_snapshots");
    let baseline_fys = h.scalar("SELECT COUNT(*) FROM fiscal_year_status");
    let baseline_domain = h.scalar("SELECT COUNT(*) FROM domain_events");
    let baseline_audit = h.scalar("SELECT COUNT(*) FROM audit_log");
    let baseline_products = h.scalar("SELECT COUNT(*) FROM products");
    let baseline_suppliers = h.scalar("SELECT COUNT(*) FROM suppliers");

    h.delete(&unit_id).expect("successful deletion");

    // Class-A rows gone
    assert!(!h.exists("SELECT COUNT(*) FROM units"));
    assert_eq!(
        h.scalar1("SELECT COUNT(*) FROM users WHERE id = ?1", &operator_id),
        0
    );
    assert_eq!(
        h.scalar1(
            "SELECT COUNT(*) FROM contracts WHERE unit_id = ?1",
            &unit_id
        ),
        0
    );
    assert_eq!(h.scalar("SELECT COUNT(*) FROM contract_products"), 0);
    assert_eq!(
        h.scalar1(
            "SELECT COUNT(*) FROM contract_allocations WHERE unit_id = ?1",
            &unit_id
        ),
        0
    );
    assert_eq!(
        h.scalar("SELECT COUNT(*) FROM contract_allocation_exceptions"),
        0
    );
    assert_eq!(
        h.scalar1(
            "SELECT COUNT(*) FROM supplier_orders WHERE unit_id = ?1",
            &unit_id
        ),
        0
    );
    assert_eq!(
        h.scalar1(
            "SELECT COUNT(*) FROM supplier_order_items WHERE unit_id = ?1",
            &unit_id
        ),
        0
    );
    assert_eq!(
        h.scalar("SELECT COUNT(*) FROM supplier_order_item_allocations"),
        0
    );
    assert_eq!(
        h.scalar1(
            "SELECT COUNT(*) FROM unit_suppliers WHERE unit_id = ?1",
            &unit_id
        ),
        0
    );
    assert_eq!(
        h.scalar1(
            "SELECT COUNT(*) FROM fifo_stock_layers WHERE unit_id = ?1",
            &unit_id
        ),
        0
    );
    assert_eq!(
        h.scalar1(
            "SELECT COUNT(*) FROM stock_movements WHERE unit_id = ?1",
            &unit_id
        ),
        0
    );
    assert_eq!(
        h.scalar1(
            "SELECT COUNT(*) FROM inventory_layer_consumptions WHERE unit_id = ?1",
            &unit_id
        ),
        0
    );
    assert_eq!(
        h.scalar1(
            "SELECT COUNT(*) FROM daily_reports WHERE unit_id = ?1",
            &unit_id
        ),
        0
    );
    assert_eq!(h.scalar("SELECT COUNT(*) FROM daily_report_meals"), 0);
    assert_eq!(h.scalar("SELECT COUNT(*) FROM daily_report_meal_items"), 0);
    assert_eq!(
        h.scalar1(
            "SELECT COUNT(*) FROM monthly_reports WHERE unit_id = ?1",
            &unit_id
        ),
        0
    );
    assert_eq!(
        h.scalar1(
            "SELECT COUNT(*) FROM unit_monthly_snapshots WHERE unit_id = ?1",
            &unit_id
        ),
        0
    );
    assert_eq!(
        h.scalar1(
            "SELECT COUNT(*) FROM identity_store WHERE subject_id = ?1",
            &unit_id
        ),
        0
    );

    // Class-B preserved verbatim
    assert_eq!(
        h.scalar("SELECT COUNT(*) FROM inventory_stocks"),
        baseline_stocks
    );
    assert_eq!(
        h.scalar("SELECT COUNT(*) FROM opening_balance_snapshots"),
        baseline_opening
    );
    assert_eq!(
        h.scalar("SELECT COUNT(*) FROM fiscal_export_snapshots"),
        baseline_export
    );
    assert_eq!(
        h.scalar("SELECT COUNT(*) FROM fiscal_year_status"),
        baseline_fys
    );
    assert_eq!(
        h.scalar("SELECT COUNT(*) FROM domain_events"),
        baseline_domain
    );
    assert_eq!(h.scalar("SELECT COUNT(*) FROM products"), baseline_products);
    assert_eq!(
        h.scalar("SELECT COUNT(*) FROM suppliers"),
        baseline_suppliers
    );
    // Exactly one audit row added: the DeleteUnit success row.
    assert_eq!(
        h.scalar("SELECT COUNT(*) FROM audit_log"),
        baseline_audit + 1
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 3 — Closed-year purge leaves fiscal_export_snapshots and fiscal_year_status
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn closed_year_data_purged_without_touching_fiscal_machinery() {
    let mut h = Harness::new("del_closed_year");
    let (unit_id, _op) = h.create_unit("T101", "Unit 101");
    {
        let c = h.conn();
        seed_product(c, "p1");
        seed_supplier(c, "s1");
        seed_fiscal_year_status(c, 2025, "closed", 1);
        seed_fiscal_export(c, "hash-closed", 2025);
        seed_contract(c, "ct1", &unit_id, "s1", 2025);
        seed_contract_product(c, "cp1", "ct1", "p1");
        seed_layer(c, "l1", &unit_id, "p1", 2025);
        seed_movement(c, "mv1", "p1", "OUT", Some(&unit_id), 2025);
    }
    let export_before = h.scalar(
        "SELECT inventory_total_value FROM fiscal_export_snapshots WHERE export_hash='hash-closed'",
    );
    let fys_count_before = h.scalar("SELECT COUNT(*) FROM fiscal_year_status");

    h.delete(&unit_id).expect("closed-year purge allowed");

    assert_eq!(
        h.scalar1(
            "SELECT COUNT(*) FROM contracts WHERE unit_id = ?1",
            &unit_id
        ),
        0
    );
    assert_eq!(
        h.scalar1(
            "SELECT COUNT(*) FROM fifo_stock_layers WHERE unit_id = ?1",
            &unit_id
        ),
        0
    );
    assert_eq!(h.scalar("SELECT COUNT(*) FROM fiscal_export_snapshots"), 1);
    assert_eq!(
        h.scalar("SELECT inventory_total_value FROM fiscal_export_snapshots WHERE export_hash='hash-closed'"),
        export_before
    );
    assert_eq!(
        h.scalar("SELECT COUNT(*) FROM fiscal_year_status"),
        fys_count_before
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 4 — NULL-owned rows are preserved (D20, §4.8)
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn null_owned_rows_are_preserved() {
    let mut h = Harness::new("del_null_owned");
    let (unit_id, _op) = h.create_unit("T102", "Unit 102");
    {
        let c = h.conn();
        seed_product(c, "p1");
        // A NULL-unit OUT movement is schema-legal; a NULL-unit report is a real
        // sync-import possibility; a NULL-unit order is preserved.
        seed_movement(c, "mv-null", "p1", "OUT", None, 2025);
        seed_movement(c, "mv-target", "p1", "OUT", Some(&unit_id), 2025);
        seed_daily_report(c, "dr-null", None, 2025);
        seed_daily_report(c, "dr-target", Some(&unit_id), 2025);
        seed_supplier(c, "s1");
        seed_order(c, "o-null", "s1", None, 2025);
        seed_order(c, "o-target", "s1", Some(&unit_id), 2025);
    }

    h.delete(&unit_id).expect("delete with NULL-owned rows");

    assert_eq!(
        h.scalar("SELECT COUNT(*) FROM stock_movements WHERE id='mv-null'"),
        1
    );
    assert_eq!(
        h.scalar("SELECT COUNT(*) FROM stock_movements WHERE id='mv-target'"),
        0
    );
    assert_eq!(
        h.scalar("SELECT COUNT(*) FROM daily_reports WHERE id='dr-null'"),
        1
    );
    assert_eq!(
        h.scalar("SELECT COUNT(*) FROM daily_reports WHERE id='dr-target'"),
        0
    );
    assert_eq!(
        h.scalar("SELECT COUNT(*) FROM supplier_orders WHERE id='o-null'"),
        1
    );
    assert_eq!(
        h.scalar("SELECT COUNT(*) FROM supplier_orders WHERE id='o-target'"),
        0
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 5 — Cross-UNIT inventory_layer_consumptions → target layer/movement
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn cross_unit_consumption_to_target_layer_blocks() {
    let mut h = Harness::new("del_ilm_layer");
    let (unit_id, _op) = h.create_unit("T103", "Unit 103");
    let (other_id, _op2) = h.create_unit("T104", "Unit 104");
    {
        let c = h.conn();
        seed_product(c, "p1");
        seed_layer(c, "l-target", &unit_id, "p1", 2025);
        seed_movement(c, "mv-other", "p1", "OUT", Some(&other_id), 2025);
        // Non-target consumption referencing a target layer.
        seed_ilm(c, "ilm-bad", &other_id, "mv-other", "l-target");
    }
    let err = h.delete(&unit_id).expect_err("must fail closed");
    assert!(is_delete_forbidden(&err), "got {err:?}");
    assert_eq!(
        h.scalar1("SELECT COUNT(*) FROM units WHERE id = ?1", &unit_id),
        1
    );
    assert_eq!(
        h.scalar("SELECT COUNT(*) FROM fifo_stock_layers WHERE id='l-target'"),
        1
    );
    assert_eq!(
        h.scalar("SELECT COUNT(*) FROM inventory_layer_consumptions WHERE id='ilm-bad'"),
        1
    );
}

#[test]
fn cross_unit_consumption_to_target_movement_blocks() {
    let mut h = Harness::new("del_ilm_mvmt");
    let (unit_id, _op) = h.create_unit("T105", "Unit 105");
    let (other_id, _op2) = h.create_unit("T106", "Unit 106");
    {
        let c = h.conn();
        seed_product(c, "p1");
        seed_movement(c, "mv-target", "p1", "OUT", Some(&unit_id), 2025);
        seed_layer(c, "l-other", &other_id, "p1", 2025);
        // Non-target consumption referencing a target movement.
        seed_ilm(c, "ilm-bad", &other_id, "mv-target", "l-other");
    }
    let err = h.delete(&unit_id).expect_err("must fail closed");
    assert!(is_delete_forbidden(&err), "got {err:?}");
    assert_eq!(
        h.scalar1("SELECT COUNT(*) FROM units WHERE id = ?1", &unit_id),
        1
    );
    assert_eq!(
        h.scalar("SELECT COUNT(*) FROM stock_movements WHERE id='mv-target'"),
        1
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 6 / 7 — Q6: target item under NULL / foreign parent order aborts
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn q6_target_item_under_null_parent_order_blocks() {
    let mut h = Harness::new("del_q6_null");
    let (unit_id, _op) = h.create_unit("T107", "Unit 107");
    {
        let c = h.conn();
        seed_product(c, "p1");
        seed_supplier(c, "s1");
        seed_order(c, "o-null", "s1", None, 2025);
        seed_order_item(c, "oi-target", "o-null", "p1", Some(&unit_id), 2025);
    }
    let err = h.delete(&unit_id).expect_err("Q6 NULL parent must abort");
    assert!(is_delete_forbidden(&err), "got {err:?}");
    // §4.7 diagnostic: conflicting item, its order, and both ownership values (NULL explicit).
    let reason = delete_forbidden_reason(&err);
    assert!(reason.contains("item=oi-target"), "reason: {reason}");
    assert!(reason.contains("order=o-null"), "reason: {reason}");
    assert!(
        reason.contains(&format!("item.unit_id={unit_id}")),
        "reason: {reason}"
    );
    assert!(reason.contains("order.unit_id=NULL"), "reason: {reason}");
    assert_eq!(
        h.scalar1("SELECT COUNT(*) FROM units WHERE id = ?1", &unit_id),
        1
    );
    assert_eq!(
        h.scalar("SELECT COUNT(*) FROM supplier_order_items WHERE id='oi-target'"),
        1
    );
    assert_eq!(
        h.scalar("SELECT COUNT(*) FROM supplier_orders WHERE id='o-null'"),
        1
    );
}

#[test]
fn q6_target_item_under_foreign_parent_order_blocks() {
    let mut h = Harness::new("del_q6_foreign");
    let (unit_id, _op) = h.create_unit("T108", "Unit 108");
    let (other_id, _op2) = h.create_unit("T109", "Unit 109");
    {
        let c = h.conn();
        seed_product(c, "p1");
        seed_supplier(c, "s1");
        seed_order(c, "o-other", "s1", Some(&other_id), 2025);
        seed_order_item(c, "oi-target", "o-other", "p1", Some(&unit_id), 2025);
    }
    let err = h
        .delete(&unit_id)
        .expect_err("Q6 foreign parent must abort");
    assert!(is_delete_forbidden(&err), "got {err:?}");
    let reason = delete_forbidden_reason(&err);
    assert!(reason.contains("item=oi-target"), "reason: {reason}");
    assert!(reason.contains("order=o-other"), "reason: {reason}");
    assert!(
        reason.contains(&format!("item.unit_id={unit_id}")),
        "reason: {reason}"
    );
    assert!(
        reason.contains(&format!("order.unit_id={other_id}")),
        "reason: {reason}"
    );
    assert_eq!(
        h.scalar1("SELECT COUNT(*) FROM units WHERE id = ?1", &unit_id),
        1
    );
    assert_eq!(
        h.scalar("SELECT COUNT(*) FROM supplier_order_items WHERE id='oi-target'"),
        1
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 8 / 9 — D4: target order with NULL / foreign child item blocks
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn d4_target_order_with_null_child_blocks() {
    let mut h = Harness::new("del_d4_null");
    let (unit_id, _op) = h.create_unit("T110", "Unit 110");
    {
        let c = h.conn();
        seed_product(c, "p1");
        seed_supplier(c, "s1");
        seed_order(c, "o-target", "s1", Some(&unit_id), 2025);
        seed_order_item(c, "oi-null", "o-target", "p1", None, 2025);
    }
    let err = h
        .delete(&unit_id)
        .expect_err("D4 NULL child must fail closed");
    assert!(is_delete_forbidden(&err), "got {err:?}");
    // §4.7 diagnostic: report the conflicting child item, its order, and both ownership
    // values (the child's NULL unit rendered explicitly).
    let reason = delete_forbidden_reason(&err);
    assert!(reason.contains("item=oi-null"), "reason: {reason}");
    assert!(reason.contains("order=o-target"), "reason: {reason}");
    assert!(reason.contains("item.unit_id=NULL"), "reason: {reason}");
    assert!(
        reason.contains(&format!("order.unit_id={unit_id}")),
        "reason: {reason}"
    );
    assert_eq!(
        h.scalar1("SELECT COUNT(*) FROM units WHERE id = ?1", &unit_id),
        1
    );
    assert_eq!(
        h.scalar("SELECT COUNT(*) FROM supplier_order_items WHERE id='oi-null'"),
        1
    );
}

#[test]
fn d4_target_order_with_foreign_child_blocks() {
    let mut h = Harness::new("del_d4_foreign");
    let (unit_id, _op) = h.create_unit("T111", "Unit 111");
    let (other_id, _op2) = h.create_unit("T112", "Unit 112");
    {
        let c = h.conn();
        seed_product(c, "p1");
        seed_supplier(c, "s1");
        seed_order(c, "o-target", "s1", Some(&unit_id), 2025);
        seed_order_item(c, "oi-other", "o-target", "p1", Some(&other_id), 2025);
    }
    let err = h
        .delete(&unit_id)
        .expect_err("D4 foreign child must fail closed");
    assert!(is_delete_forbidden(&err), "got {err:?}");
    let reason = delete_forbidden_reason(&err);
    assert!(reason.contains("item=oi-other"), "reason: {reason}");
    assert!(reason.contains("order=o-target"), "reason: {reason}");
    assert!(
        reason.contains(&format!("item.unit_id={other_id}")),
        "reason: {reason}"
    );
    assert!(
        reason.contains(&format!("order.unit_id={unit_id}")),
        "reason: {reason}"
    );
    assert_eq!(
        h.scalar1("SELECT COUNT(*) FROM units WHERE id = ?1", &unit_id),
        1
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 10 — D5: contract/allocation UNIT mismatch blocks
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn d5_contract_allocation_unit_mismatch_blocks() {
    let mut h = Harness::new("del_d5");
    let (unit_id, _op) = h.create_unit("T113", "Unit 113");
    let (other_id, _op2) = h.create_unit("T114", "Unit 114");
    {
        let c = h.conn();
        seed_product(c, "p1");
        seed_supplier(c, "s1");
        seed_contract(c, "ct1", &unit_id, "s1", 2025);
        seed_contract_product(c, "cp1", "ct1", "p1");
        // Allocation points at the target contract but carries the other unit.
        seed_allocation(c, "al-bad", "ct1", "cp1", &other_id, "p1", 2025);
    }
    let err = h.delete(&unit_id).expect_err("D5 must fail closed");
    assert!(is_delete_forbidden(&err), "got {err:?}");
    assert_eq!(
        h.scalar1("SELECT COUNT(*) FROM units WHERE id = ?1", &unit_id),
        1
    );
    assert_eq!(
        h.scalar("SELECT COUNT(*) FROM contract_allocations WHERE id='al-bad'"),
        1
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 11 — D6: two-sided supplier_order_item_allocations ownership mismatch blocks
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn d6_two_sided_ownership_mismatch_blocks() {
    let mut h = Harness::new("del_d6");
    let (unit_id, _op) = h.create_unit("T115", "Unit 115");
    let (other_id, _op2) = h.create_unit("T116", "Unit 116");
    {
        let c = h.conn();
        seed_product(c, "p1");
        seed_supplier(c, "s1");
        // Target item, foreign allocation.
        seed_order(c, "o-target", "s1", Some(&unit_id), 2025);
        seed_order_item(c, "oi-target", "o-target", "p1", Some(&unit_id), 2025);
        seed_contract(c, "ct-other", &other_id, "s1", 2025);
        seed_contract_product(c, "cp-other", "ct-other", "p1");
        seed_allocation(c, "al-other", "ct-other", "cp-other", &other_id, "p1", 2025);
        seed_soia(c, "soia-bad", "oi-target", "al-other");
    }
    let err = h.delete(&unit_id).expect_err("D6 must fail closed");
    assert!(is_delete_forbidden(&err), "got {err:?}");
    assert_eq!(
        h.scalar1("SELECT COUNT(*) FROM units WHERE id = ?1", &unit_id),
        1
    );
    assert_eq!(
        h.scalar("SELECT COUNT(*) FROM supplier_order_item_allocations WHERE id='soia-bad'"),
        1
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 12 — D14: operator with audit history is rejected; state unchanged
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn d14_operator_audit_history_blocks_and_preserves_everything() {
    let mut h = Harness::new("del_d14");
    let (unit_id, operator_id) = h.create_unit("T117", "Unit 117");
    {
        let c = h.conn();
        seed_product(c, "p1");
        seed_supplier(c, "s1");
        seed_order(c, "o1", "s1", Some(&unit_id), 2025);
        seed_order_item(c, "oi1", "o1", "p1", Some(&unit_id), 2025);
        seed_layer(c, "l1", &unit_id, "p1", 2025);
        // The gate: an audit row that references the canonical operator.
        seed_audit_for_user(c, "audit-op", &operator_id);
    }
    let audit_before = h.scalar("SELECT COUNT(*) FROM audit_log");
    let orders_before = h.scalar("SELECT COUNT(*) FROM supplier_orders");

    let err = h.delete(&unit_id).expect_err("D14 must fail closed");
    assert!(is_delete_forbidden(&err), "got {err:?}");

    assert_eq!(
        h.scalar1("SELECT COUNT(*) FROM units WHERE id = ?1", &unit_id),
        1
    );
    assert_eq!(
        h.scalar1("SELECT COUNT(*) FROM users WHERE id = ?1", &operator_id),
        1
    );
    assert_eq!(
        h.scalar1(
            "SELECT COUNT(*) FROM supplier_orders WHERE unit_id = ?1",
            &unit_id
        ),
        1
    );
    assert_eq!(
        h.scalar1(
            "SELECT COUNT(*) FROM fifo_stock_layers WHERE unit_id = ?1",
            &unit_id
        ),
        1
    );
    assert_eq!(h.scalar("SELECT COUNT(*) FROM audit_log"), audit_before);
    assert_eq!(
        h.scalar("SELECT COUNT(*) FROM supplier_orders"),
        orders_before
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 13 — Canonical operator resolution failures
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn canonical_operator_missing_blocks() {
    let mut h = Harness::new("del_canon_missing");
    let (unit_id, operator_id) = h.create_unit("T118", "Unit 118");
    h.conn()
        .execute("DELETE FROM users WHERE id = ?1", [&operator_id])
        .unwrap();
    let err = h
        .delete(&unit_id)
        .expect_err("missing operator must fail closed");
    assert!(is_delete_forbidden(&err), "got {err:?}");
    assert_eq!(
        h.scalar1("SELECT COUNT(*) FROM units WHERE id = ?1", &unit_id),
        1
    );
}

#[test]
fn canonical_operator_user_id_null_blocks() {
    let mut h = Harness::new("del_canon_null");
    let (unit_id, _op) = h.create_unit("T119", "Unit 119");
    h.conn()
        .execute("UPDATE units SET user_id = NULL WHERE id = ?1", [&unit_id])
        .unwrap();
    let err = h
        .delete(&unit_id)
        .expect_err("NULL units.user_id must fail closed");
    assert!(is_delete_forbidden(&err), "got {err:?}");
    assert_eq!(
        h.scalar1("SELECT COUNT(*) FROM units WHERE id = ?1", &unit_id),
        1
    );
}

#[test]
fn canonical_operator_user_id_mismatch_blocks() {
    let mut h = Harness::new("del_canon_mismatch");
    let (unit_id, _op) = h.create_unit("T120", "Unit 120");
    let other_id = h
        .conn()
        .query_row(
            "SELECT id FROM users WHERE username = 'admin' LIMIT 1",
            [],
            |r| r.get::<_, String>(0),
        )
        .unwrap();
    h.conn()
        .execute(
            "UPDATE units SET user_id = ?1 WHERE id = ?2",
            params![other_id, unit_id],
        )
        .unwrap();
    let err = h
        .delete(&unit_id)
        .expect_err("units.user_id mismatch must fail closed");
    assert!(is_delete_forbidden(&err), "got {err:?}");
    assert_eq!(
        h.scalar1("SELECT COUNT(*) FROM units WHERE id = ?1", &unit_id),
        1
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 14 — Surviving report item referencing a target FIFO layer blocks (§4.9)
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn surviving_report_item_referencing_target_layer_blocks() {
    let mut h = Harness::new("del_surviving_fifo");
    let (unit_id, _op) = h.create_unit("T121", "Unit 121");
    {
        let c = h.conn();
        seed_product(c, "p1");
        seed_layer(c, "l-target", &unit_id, "p1", 2025);
        // A surviving (NULL-unit) report whose item points at the target layer.
        seed_daily_report(c, "dr-surviving", None, 2025);
        seed_meal(c, "m-surviving", "dr-surviving");
        seed_meal_item(c, "mi-surviving", "m-surviving", "p1", Some("l-target"));
    }
    let err = h.delete(&unit_id).expect_err("§4.9 must fail closed");
    assert!(is_delete_forbidden(&err), "got {err:?}");
    assert_eq!(
        h.scalar1("SELECT COUNT(*) FROM units WHERE id = ?1", &unit_id),
        1
    );
    assert_eq!(
        h.scalar("SELECT COUNT(*) FROM fifo_stock_layers WHERE id='l-target'"),
        1
    );
    assert_eq!(
        h.scalar("SELECT COUNT(*) FROM daily_reports WHERE id='dr-surviving'"),
        1
    );
    assert_eq!(
        h.scalar("SELECT COUNT(*) FROM daily_report_meal_items WHERE id='mi-surviving'"),
        1
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 16 — Injected failure after purge statements begin rolls the whole thing back
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn mid_purge_failure_rolls_back_entire_operation() {
    let mut h = Harness::new("del_rollback");
    let (unit_id, operator_id) = h.create_unit("T122", "Unit 122");
    {
        let c = h.conn();
        seed_product(c, "p1");
        seed_supplier(c, "s1");
        seed_contract(c, "ct1", &unit_id, "s1", 2025);
        seed_contract_product(c, "cp1", "ct1", "p1");
        seed_allocation(c, "al1", "ct1", "cp1", &unit_id, "p1", 2025);
        seed_order(c, "o1", "s1", Some(&unit_id), 2025);
        seed_order_item(c, "oi1", "o1", "p1", Some(&unit_id), 2025);
        seed_layer(c, "l1", &unit_id, "p1", 2025);
        seed_movement(c, "mv1", "p1", "OUT", Some(&unit_id), 2025);
        seed_daily_report(c, "dr1", Some(&unit_id), 2025);
        seed_meal(c, "m1", "dr1");
        seed_meal_item(c, "mi1", "m1", "p1", None);
        seed_unit_identity(c, "id1", &unit_id);
    }
    // Trigger aborts the final A19 statement, after A1–A18 have executed.
    h.conn()
        .execute_batch(
            "CREATE TRIGGER block_units_delete BEFORE DELETE ON units \
             BEGIN SELECT RAISE(ABORT, 'injected mid-purge failure'); END;",
        )
        .unwrap();

    let err = h.delete(&unit_id).expect_err("injected failure must abort");
    assert!(
        matches!(err, AppError::Sqlite(_)) || is_delete_forbidden(&err),
        "got {err:?}"
    );

    // Full rollback: every row the purge touched is still present.
    assert_eq!(
        h.scalar1("SELECT COUNT(*) FROM units WHERE id = ?1", &unit_id),
        1
    );
    assert_eq!(
        h.scalar1("SELECT COUNT(*) FROM users WHERE id = ?1", &operator_id),
        1
    );
    assert_eq!(
        h.scalar1(
            "SELECT COUNT(*) FROM contracts WHERE unit_id = ?1",
            &unit_id
        ),
        1
    );
    assert_eq!(h.scalar("SELECT COUNT(*) FROM contract_products"), 1);
    assert_eq!(
        h.scalar1(
            "SELECT COUNT(*) FROM contract_allocations WHERE unit_id = ?1",
            &unit_id
        ),
        1
    );
    assert_eq!(
        h.scalar1(
            "SELECT COUNT(*) FROM supplier_orders WHERE unit_id = ?1",
            &unit_id
        ),
        1
    );
    assert_eq!(
        h.scalar1(
            "SELECT COUNT(*) FROM supplier_order_items WHERE unit_id = ?1",
            &unit_id
        ),
        1
    );
    assert_eq!(
        h.scalar1(
            "SELECT COUNT(*) FROM fifo_stock_layers WHERE unit_id = ?1",
            &unit_id
        ),
        1
    );
    assert_eq!(
        h.scalar1(
            "SELECT COUNT(*) FROM stock_movements WHERE unit_id = ?1",
            &unit_id
        ),
        1
    );
    assert_eq!(
        h.scalar1(
            "SELECT COUNT(*) FROM daily_reports WHERE unit_id = ?1",
            &unit_id
        ),
        1
    );
    assert_eq!(h.scalar("SELECT COUNT(*) FROM daily_report_meals"), 1);
    assert_eq!(h.scalar("SELECT COUNT(*) FROM daily_report_meal_items"), 1);
    assert_eq!(
        h.scalar1(
            "SELECT COUNT(*) FROM identity_store WHERE subject_id = ?1",
            &unit_id
        ),
        1
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 17 — Audit transaction contract on success and failure
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn success_writes_exactly_one_audit_row_failure_writes_none() {
    let mut h = Harness::new("del_audit_contract");
    let (unit_id, operator_id) = h.create_unit("T123", "Unit 123");
    h.conn()
        .execute("UPDATE units SET user_id = NULL WHERE id = ?1", [&unit_id])
        .unwrap();

    // Failed op: no audit row written (not even a Failed row).
    let before = h.scalar("SELECT COUNT(*) FROM audit_log");
    let err = h.delete(&unit_id).expect_err("must fail");
    assert!(is_delete_forbidden(&err), "got {err:?}");
    assert_eq!(h.scalar("SELECT COUNT(*) FROM audit_log"), before);

    // Repair and succeed: exactly one audit row added.
    h.conn()
        .execute(
            "UPDATE units SET user_id = ?1 WHERE id = ?2",
            params![operator_id, unit_id],
        )
        .unwrap();
    h.delete(&unit_id).expect("success after repair");
    assert_eq!(h.scalar("SELECT COUNT(*) FROM audit_log"), before + 1);
    assert_eq!(
        h.scalar1("SELECT COUNT(*) FROM units WHERE id = ?1", &unit_id),
        0
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 18 — Repeated deletion: second attempt fails closed (no tombstone)
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn repeated_deletion_fails_closed() {
    let mut h = Harness::new("del_repeat");
    let (unit_id, _op) = h.create_unit("T124", "Unit 124");
    h.delete(&unit_id).expect("first deletion succeeds");
    assert_eq!(
        h.scalar1("SELECT COUNT(*) FROM units WHERE id = ?1", &unit_id),
        0
    );

    let err = h.delete(&unit_id).expect_err("second deletion must fail");
    // Target row is gone → ResourceNotFound (a BusinessLogic error).
    assert!(matches!(err, AppError::BusinessLogic(_)), "got {err:?}");
}

// ─────────────────────────────────────────────────────────────────────────────
// §4.10 — Non-target allocation referencing a target contract_product blocks
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn cross_unit_allocation_to_target_contract_product_blocks() {
    let mut h = Harness::new("del_alloc_cp");
    let (unit_id, _op) = h.create_unit("T127", "Unit 127");
    let (other_id, _op2) = h.create_unit("T128", "Unit 128");
    {
        let c = h.conn();
        seed_product(c, "p1");
        seed_supplier(c, "s1");
        seed_contract(c, "ct-target", &unit_id, "s1", 2025);
        seed_contract_product(c, "cp-target", "ct-target", "p1");
        seed_contract(c, "ct-other", &other_id, "s1", 2025);
        seed_contract_product(c, "cp-other", "ct-other", "p1");
        // Non-target allocation whose contract_id is non-target but whose
        // contract_product_id references a target contract's product line.
        seed_allocation(c, "al-bad", "ct-other", "cp-target", &other_id, "p1", 2025);
    }
    let err = h.delete(&unit_id).expect_err("§4.10 must fail closed");
    assert!(is_delete_forbidden(&err), "got {err:?}");
    assert_eq!(
        h.scalar1("SELECT COUNT(*) FROM units WHERE id = ?1", &unit_id),
        1
    );
    assert_eq!(
        h.scalar("SELECT COUNT(*) FROM contract_allocations WHERE id='al-bad'"),
        1
    );
    assert_eq!(
        h.scalar("SELECT COUNT(*) FROM contract_products WHERE id='cp-target'"),
        1
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 19 — Legitimate data for every ownership rule is not blocked
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn legitimate_ownership_shapes_are_not_blocked() {
    let mut h = Harness::new("del_legit");
    let (unit_id, _op) = h.create_unit("T125", "Unit 125");
    let (other_id, _op2) = h.create_unit("T126", "Unit 126");
    {
        let c = h.conn();
        seed_product(c, "p1");
        seed_product(c, "p2");
        seed_supplier(c, "s1");
        // Two-sided target allocation join (safe).
        seed_contract(c, "ct1", &unit_id, "s1", 2025);
        seed_contract_product(c, "cp1", "ct1", "p1");
        seed_allocation(c, "al1", "ct1", "cp1", &unit_id, "p1", 2025);
        seed_order(c, "o1", "s1", Some(&unit_id), 2025);
        seed_order_item(c, "oi1", "o1", "p1", Some(&unit_id), 2025);
        seed_soia(c, "soia1", "oi1", "al1");
        // A different unit's fully-consistent allocation join (untouched).
        seed_contract(c, "ct2", &other_id, "s1", 2025);
        seed_contract_product(c, "cp2", "ct2", "p2");
        seed_allocation(c, "al2", "ct2", "cp2", &other_id, "p2", 2025);
        seed_order(c, "o2", "s1", Some(&other_id), 2025);
        seed_order_item(c, "oi2", "o2", "p2", Some(&other_id), 2025);
        seed_soia(c, "soia2", "oi2", "al2");
        // Same-unit report item FIFO reference (safe — deleted with target).
        seed_layer(c, "l1", &unit_id, "p1", 2025);
        seed_movement(c, "mv1", "p1", "OUT", Some(&unit_id), 2025);
        seed_ilm(c, "ilm1", &unit_id, "mv1", "l1");
        seed_daily_report(c, "dr1", Some(&unit_id), 2025);
        seed_meal(c, "m1", "dr1");
        seed_meal_item(c, "mi1", "m1", "p1", Some("l1"));
    }

    h.delete(&unit_id)
        .expect("consistent data must be deletable");

    // Target subtree gone.
    assert_eq!(
        h.scalar("SELECT COUNT(*) FROM supplier_order_item_allocations WHERE id='soia1'"),
        0
    );
    assert_eq!(
        h.scalar1(
            "SELECT COUNT(*) FROM contracts WHERE unit_id = ?1",
            &unit_id
        ),
        0
    );
    // The other UNIT's fully-consistent subtree is untouched.
    assert_eq!(
        h.scalar1(
            "SELECT COUNT(*) FROM contracts WHERE unit_id = ?1",
            &other_id
        ),
        1
    );
    assert_eq!(
        h.scalar("SELECT COUNT(*) FROM contract_allocations WHERE id='al2'"),
        1
    );
    assert_eq!(
        h.scalar("SELECT COUNT(*) FROM supplier_order_item_allocations WHERE id='soia2'"),
        1
    );
    assert_eq!(
        h.scalar("SELECT COUNT(*) FROM supplier_orders WHERE id='o2'"),
        1
    );
    assert_eq!(
        h.scalar1("SELECT COUNT(*) FROM units WHERE id = ?1", &other_id),
        1
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 20 — Soft-deleted canonical operator is still resolved and hard-deleted (D1.1)
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn soft_deleted_canonical_operator_is_resolved_and_purged() {
    let mut h = Harness::new("del_canon_soft_deleted");
    let (unit_id, operator_id) = h.create_unit("T129", "Unit 129");
    // ADR-0064 D1.1: resolution MUST use the raw (soft-delete-inclusive) helper.
    // A regression to the active-only `get_user_by_username` would return None and
    // this deletion would fail closed instead of succeeding.
    h.conn()
        .execute("UPDATE users SET deleted = 1 WHERE id = ?1", [&operator_id])
        .unwrap();
    assert_eq!(
        h.scalar1("SELECT COUNT(*) FROM users WHERE id = ?1", &operator_id),
        1
    );
    assert_eq!(
        h.scalar1("SELECT deleted FROM users WHERE id = ?1", &operator_id),
        1
    );

    h.delete(&unit_id)
        .expect("soft-deleted canonical operator must not block deletion");

    assert_eq!(
        h.scalar1("SELECT COUNT(*) FROM units WHERE id = ?1", &unit_id),
        0
    );
    assert_eq!(
        h.scalar1("SELECT COUNT(*) FROM users WHERE id = ?1", &operator_id),
        0
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 21 — D6 second direction: target allocation joined to a NULL-owned item
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn d6_target_allocation_with_null_item_blocks() {
    let mut h = Harness::new("del_d6_reverse");
    let (unit_id, _op) = h.create_unit("T130", "Unit 130");
    {
        let c = h.conn();
        seed_product(c, "p1");
        seed_supplier(c, "s1");
        // Target-owned allocation.
        seed_contract(c, "ct-target", &unit_id, "s1", 2025);
        seed_contract_product(c, "cp-target", "ct-target", "p1");
        seed_allocation(
            c,
            "al-target",
            "ct-target",
            "cp-target",
            &unit_id,
            "p1",
            2025,
        );
        // Surviving NULL-owned order + item (own unit_id NULL), joined to the target
        // allocation. D6's second disjunct (`ca.unit_id = target AND soi.unit_id IS NULL`)
        // must reject before any DELETE; Q6/D4 do not fire because neither side is target.
        seed_order(c, "o-null", "s1", None, 2025);
        seed_order_item(c, "oi-null", "o-null", "p1", None, 2025);
        seed_soia(c, "soia-bad", "oi-null", "al-target");
    }
    let err = h
        .delete(&unit_id)
        .expect_err("D6 second direction must fail closed");
    assert!(is_delete_forbidden(&err), "got {err:?}");
    let reason = delete_forbidden_reason(&err);
    assert!(
        reason.contains("cannot prove two-sided target ownership"),
        "reason: {reason}"
    );
    assert!(reason.contains("soia-bad"), "reason: {reason}");
    assert_eq!(
        h.scalar1("SELECT COUNT(*) FROM units WHERE id = ?1", &unit_id),
        1
    );
    assert_eq!(
        h.scalar("SELECT COUNT(*) FROM supplier_order_item_allocations WHERE id='soia-bad'"),
        1
    );
    assert_eq!(
        h.scalar("SELECT COUNT(*) FROM contract_allocations WHERE id='al-target'"),
        1
    );
    assert_eq!(
        h.scalar("SELECT COUNT(*) FROM supplier_order_items WHERE id='oi-null'"),
        1
    );
    assert_eq!(
        h.scalar("SELECT COUNT(*) FROM supplier_orders WHERE id='o-null'"),
        1
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 22 — D5 reverse direction: target allocation under another UNIT's contract
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn d5_target_allocation_with_foreign_parent_contract_blocks() {
    let mut h = Harness::new("del_d5_reverse");
    let (unit_id, _op) = h.create_unit("T132", "Unit 132");
    let (other_id, _op2) = h.create_unit("T133", "Unit 133");
    {
        let c = h.conn();
        seed_product(c, "p1");
        seed_supplier(c, "s1");
        // Target-owned allocation whose parent contract belongs to another UNIT.
        seed_contract(c, "ct-other", &other_id, "s1", 2025);
        seed_contract_product(c, "cp-other", "ct-other", "p1");
        seed_allocation(c, "al-bad", "ct-other", "cp-other", &unit_id, "p1", 2025);
    }
    let err = h
        .delete(&unit_id)
        .expect_err("D5 reverse direction must fail closed");
    assert!(is_delete_forbidden(&err), "got {err:?}");
    let reason = delete_forbidden_reason(&err);
    assert!(
        reason.contains("disagree with their parent contract unit"),
        "reason: {reason}"
    );
    assert!(reason.contains("al-bad"), "reason: {reason}");
    assert_eq!(
        h.scalar1("SELECT COUNT(*) FROM units WHERE id = ?1", &unit_id),
        1
    );
    assert_eq!(
        h.scalar("SELECT COUNT(*) FROM contract_allocations WHERE id='al-bad'"),
        1
    );
    assert_eq!(
        h.scalar1(
            "SELECT COUNT(*) FROM contracts WHERE unit_id = ?1",
            &other_id
        ),
        1
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 23 — §4.10 structural mismatch: target allocation referencing a target
//      contract_product from a *different* target contract
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn target_owned_allocation_structural_mismatch_blocks() {
    let mut h = Harness::new("del_alloc_cp_struct");
    let (unit_id, _op) = h.create_unit("T134", "Unit 134");
    {
        let c = h.conn();
        seed_product(c, "p1");
        seed_supplier(c, "s1");
        // Two target-owned contracts in different fiscal years (a UNIT has at most one
        // live contract per fiscal year). The allocation is target-owned and its parent
        // contract (ct-b) is target-owned, so D5 is satisfied; but its
        // `contract_product_id` references a line of the *other* target contract
        // (ct-a), which is exactly §4.10 row 5's structural-mismatch condition and
        // must fail closed before any DELETE.
        seed_contract(c, "ct-a", &unit_id, "s1", 2025);
        seed_contract_product(c, "cp-a", "ct-a", "p1");
        seed_contract(c, "ct-b", &unit_id, "s1", 2026);
        seed_allocation(c, "al-bad", "ct-b", "cp-a", &unit_id, "p1", 2026);
    }
    let err = h
        .delete(&unit_id)
        .expect_err("§4.10 structural mismatch must fail closed");
    assert!(is_delete_forbidden(&err), "got {err:?}");
    // Pin the §4.10 check specifically (not the D5 check) via its message.
    let reason = delete_forbidden_reason(&err);
    assert!(
        reason.contains("reference target contract products unsafely"),
        "reason: {reason}"
    );
    assert!(reason.contains("al-bad"), "reason: {reason}");
    assert_eq!(
        h.scalar1("SELECT COUNT(*) FROM units WHERE id = ?1", &unit_id),
        1
    );
    assert_eq!(
        h.scalar("SELECT COUNT(*) FROM contract_allocations WHERE id='al-bad'"),
        1
    );
    assert_eq!(
        h.scalar("SELECT COUNT(*) FROM contract_products WHERE id='cp-a'"),
        1
    );
    assert_eq!(
        h.scalar1(
            "SELECT COUNT(*) FROM contracts WHERE unit_id = ?1",
            &unit_id
        ),
        2
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 24 — A17 removes only the target UNIT identity row
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn deletion_preserves_non_unit_and_other_unit_identity_records() {
    let mut h = Harness::new("del_identity_preserve");
    let (unit_id, _op) = h.create_unit("T135", "Unit 135");
    let (other_id, _op2) = h.create_unit("T136", "Unit 136");
    {
        let c = h.conn();
        seed_identity(c, "id-target", "UNIT", &unit_id);
        seed_identity(c, "id-other-unit", "UNIT", &other_id);
        seed_identity(c, "id-wilaya", "WILAYA", "WILAYA");
        seed_identity(c, "id-admin", "ADMIN", "admin");
    }

    h.delete(&unit_id).expect("target deletion");

    // A17 predicate is `subject_type = 'UNIT' AND subject_id = <target>`: only the
    // target UNIT identity row is removed; WILAYA, ADMIN, and the other UNIT survive.
    assert_eq!(
        h.scalar("SELECT COUNT(*) FROM identity_store WHERE identity_id='id-target'"),
        0
    );
    assert_eq!(
        h.scalar("SELECT COUNT(*) FROM identity_store WHERE identity_id='id-other-unit'"),
        1
    );
    assert_eq!(
        h.scalar("SELECT COUNT(*) FROM identity_store WHERE identity_id='id-wilaya'"),
        1
    );
    assert_eq!(
        h.scalar("SELECT COUNT(*) FROM identity_store WHERE identity_id='id-admin'"),
        1
    );
    assert_eq!(
        h.scalar1("SELECT COUNT(*) FROM units WHERE id = ?1", &other_id),
        1
    );
}
