-- =============================================================================
-- GRPC SQLite Schema Baseline (Consolidated)
-- Version: 1.0.0 (Baseline)
-- Generated: 2026-05-16
-- =============================================================================

-- =============================================================================
-- 1. CORE SETTINGS & USERS
-- =============================================================================

CREATE TABLE IF NOT EXISTS settings (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    node_type TEXT NOT NULL DEFAULT 'UNCONFIGURED',
    unit_name TEXT,
    current_year INTEGER NOT NULL DEFAULT 2024,
    wilaya_code TEXT,
    wilaya_name TEXT,
    configured BOOLEAN NOT NULL DEFAULT 0
);

-- Canonical account identity (ADR-0052): username is unique per node scope,
-- not globally. UNIT operators are all named 'user' (one per unit code);
-- the fleet admin is a single ('admin', <local scope>) row per database.
CREATE TABLE IF NOT EXISTS users (
    id TEXT PRIMARY KEY,
    username TEXT NOT NULL,
    password_hash TEXT NOT NULL,
    role TEXT NOT NULL DEFAULT 'User',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    node_id TEXT NOT NULL DEFAULT 'WILAYA',
    deleted INTEGER NOT NULL DEFAULT 0 CHECK(deleted IN (0, 1)),
    UNIQUE(username, node_id)
);

CREATE TABLE IF NOT EXISTS units (
    id TEXT PRIMARY KEY,
    code TEXT UNIQUE NOT NULL,
    name TEXT NOT NULL,
    wilaya_code TEXT NOT NULL,
    user_id TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    node_id TEXT,
    deleted INTEGER NOT NULL DEFAULT 0 CHECK(deleted IN (0, 1)),
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE SET NULL
);

-- =============================================================================
-- 2. FISCAL & PERIOD MANAGEMENT
-- =============================================================================

CREATE TABLE IF NOT EXISTS fiscal_year_status (
    year INTEGER PRIMARY KEY,
    status TEXT NOT NULL DEFAULT 'open' CHECK(status IN ('open', 'closed')),
    opened_at TEXT,
    closed_at TEXT,
    closed_by TEXT,
    archived INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS applied_fiscal_transitions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    fiscal_transition_id TEXT NOT NULL UNIQUE,
    closed_year INTEGER NOT NULL,
    opened_year INTEGER NOT NULL,
    applied_at TEXT NOT NULL,
    applied_by TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS fiscal_closure_package_registry (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    transition_id TEXT NOT NULL UNIQUE,
    fiscal_year INTEGER NOT NULL,
    next_year INTEGER NOT NULL,
    package_fingerprint TEXT NOT NULL,
    signing_key_id TEXT NOT NULL,
    exported_at TEXT NOT NULL,
    applied_at TEXT,
    retention_status TEXT NOT NULL CHECK(retention_status IN ('ACTIVE','ARCHIVED','RETIRED')),
    exported_by TEXT NOT NULL,
    applied_by TEXT,
    archived INTEGER NOT NULL DEFAULT 0,
    notes TEXT
);

-- =============================================================================
-- 3. PRODUCT CATALOG & INVENTORY
-- =============================================================================

CREATE TABLE IF NOT EXISTS products (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    base_price REAL NOT NULL CHECK (base_price >= 0),
    tva REAL NOT NULL DEFAULT 0.0 CHECK (tva >= 0 AND tva <= 100),
    supplier_name TEXT,
    year INTEGER NOT NULL CHECK (year >= 2020 AND year <= 2100),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    node_id TEXT,
    deleted INTEGER NOT NULL DEFAULT 0 CHECK(deleted IN (0, 1))
);

CREATE TABLE IF NOT EXISTS inventory_stocks (
    id TEXT PRIMARY KEY,
    product_id TEXT NOT NULL UNIQUE,
    quantity REAL NOT NULL DEFAULT 0.0 CHECK (quantity >= 0),
    unit TEXT DEFAULT 'unit',
    last_updated TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    node_id TEXT,
    deleted INTEGER NOT NULL DEFAULT 0 CHECK(deleted IN (0, 1)),
    FOREIGN KEY (product_id) REFERENCES products(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS opening_balance_snapshots (
    id TEXT PRIMARY KEY,
    product_id TEXT NOT NULL,
    fiscal_year INTEGER NOT NULL,
    opening_quantity REAL NOT NULL,
    carried_from_year INTEGER,
    created_at TEXT NOT NULL,
    created_by TEXT NOT NULL,
    unit_cost REAL NOT NULL DEFAULT 0 CHECK(unit_cost >= 0),
    total_value REAL NOT NULL DEFAULT 0 CHECK(total_value >= 0),
    snapshot_reason TEXT NOT NULL DEFAULT 'year_close',
    UNIQUE(product_id, fiscal_year)
);

CREATE TABLE IF NOT EXISTS stock_movements (
    id TEXT PRIMARY KEY,
    product_id TEXT NOT NULL,
    movement_type TEXT NOT NULL CHECK(movement_type IN ('IN', 'OUT', 'OPENING')),
    quantity REAL NOT NULL CHECK(quantity > 0),
    balance_before REAL NOT NULL CHECK(balance_before >= 0),
    balance_after REAL NOT NULL CHECK(balance_after >= 0),
    reference_type TEXT CHECK(reference_type IN ('Order', 'Consumption', 'Opening', NULL)),
    reference_id TEXT,
    notes TEXT,
    timestamp TEXT NOT NULL,
    user_id TEXT NOT NULL,
    username TEXT NOT NULL,
    unit_id TEXT,
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    deleted INTEGER NOT NULL DEFAULT 0 CHECK(deleted IN (0, 1)),
    fiscal_year INTEGER,
    unit_cost REAL CHECK (unit_cost >= 0),
    FOREIGN KEY (product_id) REFERENCES products(id) ON DELETE RESTRICT,
    CHECK (movement_type != 'IN' OR unit_id IS NOT NULL)
);

CREATE TABLE IF NOT EXISTS fifo_stock_layers (
    id TEXT PRIMARY KEY,
    unit_id TEXT NOT NULL
        REFERENCES units(id) ON DELETE RESTRICT,
    product_id TEXT NOT NULL
        REFERENCES products(id) ON DELETE RESTRICT,
    source_type TEXT NOT NULL CHECK(source_type IN ('ORDER', 'OPENING')),
    source_id TEXT,
    unit_cost REAL NOT NULL CHECK(unit_cost >= 0),
    qty_original REAL NOT NULL CHECK(qty_original > 0),
    qty_remaining REAL NOT NULL CHECK(qty_remaining >= 0),
    received_at TEXT NOT NULL,
    created_by TEXT NOT NULL,
    origin_fiscal_year INTEGER NOT NULL DEFAULT 0,
    CHECK(qty_remaining <= qty_original)
);

CREATE TABLE IF NOT EXISTS inventory_layer_consumptions (
    id TEXT PRIMARY KEY,
    unit_id TEXT NOT NULL
        REFERENCES units(id) ON DELETE RESTRICT,
    movement_id TEXT NOT NULL
        REFERENCES stock_movements(id),
    layer_id TEXT NOT NULL
        REFERENCES fifo_stock_layers(id),
    quantity REAL NOT NULL CHECK(quantity > 0),
    unit_cost REAL NOT NULL CHECK(unit_cost >= 0),
    total_cost REAL NOT NULL CHECK(total_cost >= 0),
    consumed_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS reference_price_snapshots (
    id TEXT PRIMARY KEY,
    product_id TEXT NOT NULL
        REFERENCES products(id) ON DELETE RESTRICT,
    fiscal_year INTEGER NOT NULL,
    reference_price REAL NOT NULL CHECK(reference_price >= 0),
    approved_by TEXT NOT NULL,
    approved_at TEXT NOT NULL,
    UNIQUE(product_id, fiscal_year)
);

-- =============================================================================
-- 4. OPERATIONAL DOCUMENTS (ORDERS & CONSUMPTION)
-- =============================================================================

CREATE TABLE IF NOT EXISTS daily_reports (
    id TEXT PRIMARY KEY,
    date TEXT NOT NULL,
    unit_id TEXT,
    total_daily_cost REAL NOT NULL DEFAULT 0 CHECK (total_daily_cost >= 0),
    total_daily_average REAL NOT NULL DEFAULT 0 CHECK (total_daily_average >= 0),
    total_daily_beneficiaries INTEGER NOT NULL DEFAULT 0 CHECK (total_daily_beneficiaries >= 0),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    node_id TEXT,
    deleted INTEGER NOT NULL DEFAULT 0 CHECK(deleted IN (0, 1)),
    fiscal_year INTEGER
);

CREATE TABLE IF NOT EXISTS daily_report_meals (
    id TEXT PRIMARY KEY,
    daily_report_id TEXT NOT NULL,
    meal_type TEXT NOT NULL CHECK (meal_type IN ('breakfast', 'lunch', 'dinner')),
    staff_24h_count INTEGER NOT NULL DEFAULT 0 CHECK (staff_24h_count >= 0),
    staff_8h_count INTEGER NOT NULL DEFAULT 0 CHECK (staff_8h_count >= 0),
    reservation_count INTEGER NOT NULL DEFAULT 0 CHECK (reservation_count >= 0),
    mission_count INTEGER NOT NULL DEFAULT 0 CHECK (mission_count >= 0),
    guest_count INTEGER NOT NULL DEFAULT 0 CHECK (guest_count >= 0),
    total_beneficiaries INTEGER NOT NULL DEFAULT 0 CHECK (total_beneficiaries >= 0),
    total_meal_cost REAL NOT NULL DEFAULT 0 CHECK (total_meal_cost >= 0),
    meal_average REAL NOT NULL DEFAULT 0 CHECK (meal_average >= 0),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    node_id TEXT,
    deleted INTEGER NOT NULL DEFAULT 0 CHECK(deleted IN (0, 1)),
    FOREIGN KEY (daily_report_id) REFERENCES daily_reports(id) ON DELETE CASCADE,
    UNIQUE(daily_report_id, meal_type)
);

CREATE TABLE IF NOT EXISTS daily_report_meal_items (
    id TEXT PRIMARY KEY,
    meal_id TEXT NOT NULL,
    product_id TEXT NOT NULL,
    quantity REAL NOT NULL CHECK (quantity > 0),
    unit_price REAL NOT NULL CHECK (unit_price >= 0),
    total_cost REAL NOT NULL,
    fifo_layer_id TEXT,
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    node_id TEXT,
    deleted INTEGER NOT NULL DEFAULT 0 CHECK(deleted IN (0, 1)),
    FOREIGN KEY (meal_id) REFERENCES daily_report_meals(id) ON DELETE CASCADE,
    FOREIGN KEY (product_id) REFERENCES products(id)
);

CREATE TABLE IF NOT EXISTS supplier_orders (
    id TEXT PRIMARY KEY,
    order_date TEXT NOT NULL,
    supplier_name TEXT NOT NULL,
    reference_number TEXT,
    total_amount REAL,
    status TEXT NOT NULL DEFAULT 'Draft',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    node_id TEXT,
    deleted INTEGER NOT NULL DEFAULT 0 CHECK(deleted IN (0, 1)),
    fiscal_year INTEGER
);

CREATE TABLE IF NOT EXISTS supplier_order_items (
    id TEXT PRIMARY KEY,
    order_id TEXT NOT NULL,
    product_id TEXT NOT NULL,
    quantity REAL NOT NULL CHECK (quantity > 0),
    unit_price REAL NOT NULL CHECK (unit_price >= 0),
    total_cost REAL NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    node_id TEXT,
    deleted INTEGER NOT NULL DEFAULT 0 CHECK(deleted IN (0, 1)),
    FOREIGN KEY (order_id) REFERENCES supplier_orders(id) ON DELETE CASCADE,
    FOREIGN KEY (product_id) REFERENCES products(id)
);

-- =============================================================================
-- 5. ANALYTICS & REPORTING
-- =============================================================================

CREATE TABLE IF NOT EXISTS unit_monthly_snapshots (
    id TEXT PRIMARY KEY,
    unit_id TEXT NOT NULL,
    unit_name TEXT NOT NULL,
    report_year INTEGER NOT NULL CHECK(report_year >= 2020),
    report_month INTEGER NOT NULL CHECK(report_month BETWEEN 1 AND 12),
    product_id TEXT NOT NULL,
    product_name TEXT NOT NULL,
    opening_stock REAL NOT NULL DEFAULT 0.0 CHECK(opening_stock >= 0),
    total_in REAL NOT NULL DEFAULT 0.0 CHECK(total_in >= 0),
    total_out REAL NOT NULL DEFAULT 0.0 CHECK(total_out >= 0),
    computed_closing REAL NOT NULL DEFAULT 0.0,
    reported_closing REAL NOT NULL DEFAULT 0.0 CHECK(reported_closing >= 0),
    variance REAL NOT NULL DEFAULT 0.0,
    has_balance_anomaly INTEGER NOT NULL DEFAULT 0,
    avg_consumption_3months REAL,
    has_consumption_anomaly INTEGER NOT NULL DEFAULT 0,
    is_stale INTEGER NOT NULL DEFAULT 0,
    computed_at TEXT NOT NULL,
    FOREIGN KEY (unit_id) REFERENCES units(id) ON DELETE CASCADE,
    FOREIGN KEY (product_id) REFERENCES products(id) ON DELETE CASCADE,
    UNIQUE(unit_id, report_year, report_month, product_id)
);

CREATE TABLE IF NOT EXISTS monthly_reports (
    id TEXT PRIMARY KEY,
    unit_id TEXT NOT NULL,
    report_year INTEGER NOT NULL CHECK(report_year >= 2020),
    report_month INTEGER NOT NULL CHECK(report_month BETWEEN 1 AND 12),
    total_beneficiaries INTEGER NOT NULL DEFAULT 0 CHECK(total_beneficiaries >= 0),
    total_consumption_value REAL NOT NULL DEFAULT 0.0 CHECK(total_consumption_value >= 0),
    breakfast_average REAL NOT NULL DEFAULT 0.0 CHECK(breakfast_average >= 0),
    lunch_average REAL NOT NULL DEFAULT 0.0 CHECK(lunch_average >= 0),
    dinner_average REAL NOT NULL DEFAULT 0.0 CHECK(dinner_average >= 0),
    daily_average REAL NOT NULL DEFAULT 0.0 CHECK(daily_average >= 0),
    report_count INTEGER NOT NULL DEFAULT 0 CHECK(report_count >= 0),
    imported_at TEXT NOT NULL,
    imported_by TEXT NOT NULL,
    file_hash TEXT,
    UNIQUE(unit_id, report_year, report_month)
);

CREATE TABLE IF NOT EXISTS report_generation_metadata (
    id TEXT PRIMARY KEY,
    report_type TEXT NOT NULL,
    generated_at TEXT NOT NULL,
    generated_by TEXT NOT NULL,
    fiscal_year INTEGER NOT NULL,
    inventory_valuation_method TEXT NOT NULL,
    product_count INTEGER NOT NULL,
    movement_count INTEGER NOT NULL
);

-- =============================================================================
-- 6. SYNC & AUDIT TRAIL
-- =============================================================================

CREATE TABLE IF NOT EXISTS audit_log (
    id TEXT PRIMARY KEY,
    user_id TEXT REFERENCES users(id) ON DELETE SET NULL,
    username TEXT NOT NULL,
    action TEXT NOT NULL,
    entity_type TEXT NOT NULL,
    entity_id TEXT,
    entity_name TEXT,
    old_value TEXT,
    new_value TEXT,
    session_id TEXT,
    timestamp TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'Success' CHECK(status IN ('Success', 'Failed')),
    error_message TEXT,
    metadata TEXT,
    previous_hash TEXT,
    entry_hash TEXT,
    event_type TEXT,
    actor_id TEXT,
    target_type TEXT,
    target_id TEXT,
    fiscal_year INTEGER,
    before_snapshot TEXT,
    after_snapshot TEXT,
    node_id TEXT,
    details TEXT
);

CREATE TABLE IF NOT EXISTS audit_summary (
    date TEXT PRIMARY KEY,
    total_operations INTEGER NOT NULL DEFAULT 0,
    failed_operations INTEGER NOT NULL DEFAULT 0,
    users_active INTEGER NOT NULL DEFAULT 0,
    last_updated TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS import_audit_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    event_type TEXT NOT NULL,
    package_id TEXT NOT NULL,
    package_kind TEXT NOT NULL,
    source_node_id TEXT,
    reason_code TEXT,
    occurred_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS applied_sync_packages (
    package_id TEXT PRIMARY KEY NOT NULL,
    kind TEXT NOT NULL,
    source_node_id TEXT,
    imported_at TEXT NOT NULL DEFAULT (datetime('now')),
    imported_by TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS sync_conflicts (
    id TEXT PRIMARY KEY,
    package_id TEXT NOT NULL,
    source_node_id TEXT NOT NULL,
    target_node_id TEXT NOT NULL,
    conflict_type TEXT NOT NULL,
    severity TEXT NOT NULL,
    description TEXT NOT NULL,
    suggested_resolution TEXT,
    resolved INTEGER NOT NULL DEFAULT 0,
    resolution_note TEXT,
    resolved_by TEXT,
    resolved_at TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE TABLE IF NOT EXISTS import_reproducibility_metadata (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    package_id TEXT NOT NULL,
    package_kind TEXT NOT NULL,
    imported_at TEXT NOT NULL,
    imported_by TEXT NOT NULL,
    validation_state TEXT NOT NULL,
    source_integrity_state TEXT,
    rejected_records_count INTEGER NOT NULL DEFAULT 0 CHECK (rejected_records_count >= 0)
);

-- =============================================================================
-- 6B. IDENTITY STORE (consolidated from migrations 002/003/008)
-- RFC 2026-08-04-node-identity-trust / ADR-0038 / ADR-0039 / SEC-002
--
-- The Identity Store is the single source of truth for identity state.
-- Signing/verification material (public keys) is stored as an opaque BLOB;
-- all cryptographic operations live in infrastructure/security/identity.
--
-- The `signature` column (previously migration 003) is incorporated directly
-- into the CREATE TABLE: it is NULL ONLY during the legacy migration window
-- (records issued before Identity Trust activation may carry no signature);
-- because this is a pre-release consolidated baseline, NULL remains the
-- migration mechanism until V1 support is removed (ADR-0039 §6). It is an
-- opaque Ed25519 BLOB (64 bytes); the domain enforces the fixed length at the
-- boundary via Ed25519CertificateSignature.
--
-- Single-ACTIVE-per-subject (Invariant 6) and single-ACTIVE-ADMIN (SEC-002)
-- are enforced at the DATABASE level by the partial unique indexes created in
-- section 8 (identity_store indexes).
-- =============================================================================

CREATE TABLE IF NOT EXISTS identity_store (
    identity_id TEXT PRIMARY KEY,
    subject_type TEXT NOT NULL CHECK(subject_type IN ('WILAYA', 'UNIT', 'ADMIN')),
    subject_id TEXT NOT NULL,
    issuer_identity_id TEXT,
    credential_id TEXT NOT NULL,
    generation INTEGER NOT NULL CHECK(generation >= 1),
    status TEXT NOT NULL CHECK(status IN ('ACTIVE', 'REVOKED', 'SUPERSEDED', 'EXPIRED')),
    public_key BLOB NOT NULL,
    algorithm_version INTEGER NOT NULL,
    signature BLOB,
    not_after TEXT,
    package_sequence INTEGER,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    deleted INTEGER NOT NULL DEFAULT 0 CHECK(deleted IN (0, 1))
);

-- =============================================================================
-- 6C. REGISTRY FLEET-STATE SNAPSHOTS (consolidated from migration 005)
-- RFC 2026-08-04-node-identity-trust §3.9 / B4
--
-- Stores each accepted Registry Package payload for auditability (P4) and
-- deterministic replay. Unit-management mutation stays out of scope.
-- =============================================================================

CREATE TABLE IF NOT EXISTS registry_snapshots (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    package_id TEXT NOT NULL UNIQUE,
    snapshot_version INTEGER NOT NULL,
    wilaya_identity_id TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    imported_at TEXT NOT NULL,
    imported_by TEXT NOT NULL
);

-- =============================================================================
-- 7. OBSERVABILITY & TELEMETRY
-- =============================================================================

CREATE TABLE IF NOT EXISTS fiscal_export_snapshots (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    export_hash TEXT NOT NULL UNIQUE,
    generated_at TEXT NOT NULL,
    generated_by TEXT NOT NULL,
    fiscal_year INTEGER NOT NULL,
    movement_count INTEGER NOT NULL,
    report_count INTEGER NOT NULL,
    inventory_total_value REAL NOT NULL,
    integrity_state TEXT,
    archived_years_count INTEGER,
    active_anomalies_count INTEGER,
    signing_key_id TEXT,
    export_reason TEXT
);

CREATE TABLE IF NOT EXISTS fiscal_operational_snapshots (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    snapshot_date TEXT NOT NULL,
    fiscal_year INTEGER NOT NULL,
    total_inventory_value REAL NOT NULL DEFAULT 0 CHECK (total_inventory_value >= 0),
    product_count INTEGER NOT NULL DEFAULT 0 CHECK (product_count >= 0),
    movement_count INTEGER NOT NULL DEFAULT 0 CHECK (movement_count >= 0),
    report_count INTEGER NOT NULL DEFAULT 0 CHECK (report_count >= 0),
    integrity_state TEXT NOT NULL CHECK (integrity_state IN ('OK','WARNINGS','CRITICAL','UNKNOWN')),
    created_by TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS integrity_verification_attempts (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    attempted_at TEXT NOT NULL,
    verification_type TEXT NOT NULL CHECK (verification_type IN ('INVENTORY','FISCAL_DRIFT','AUDIT_CHAIN','BACKUP_FILE','DATABASE_INTEGRITY')),
    outcome TEXT NOT NULL CHECK (outcome IN ('PASS','FAIL')),
    details TEXT
);

CREATE TABLE IF NOT EXISTS operational_findings_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    emitted_at TEXT NOT NULL,
    severity TEXT NOT NULL CHECK (severity IN ('INFO','WARNING','CRITICAL')),
    category TEXT NOT NULL,
    code TEXT NOT NULL,
    message TEXT NOT NULL,
    recommendation TEXT NOT NULL,
    context_json TEXT
);

CREATE TABLE IF NOT EXISTS operational_sessions (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL,
    username TEXT NOT NULL,
    started_at TEXT NOT NULL,
    ended_at TEXT,
    end_reason TEXT CHECK (end_reason IS NULL OR end_reason IN ('LOGOUT', 'UNEXPECTED_TERMINATION')),
    critical_operations_count INTEGER NOT NULL DEFAULT 0 CHECK (critical_operations_count >= 0),
    integrity_warnings_count INTEGER NOT NULL DEFAULT 0 CHECK (integrity_warnings_count >= 0),
    anomalies_surfaced_count INTEGER NOT NULL DEFAULT 0 CHECK (anomalies_surfaced_count >= 0)
);

CREATE TABLE IF NOT EXISTS telemetry_events (
    id TEXT PRIMARY KEY,
    event_type TEXT NOT NULL,
    outcome TEXT NOT NULL,
    duration_ms INTEGER,
    timestamp TEXT NOT NULL DEFAULT (datetime('now')),
    metadata TEXT,
    user_id TEXT,
    schema_version INTEGER NOT NULL
);

-- =============================================================================
-- 8. INDEXES (PERFORMANCE & INTEGRITY)
-- =============================================================================

-- Daily Reports
CREATE INDEX IF NOT EXISTS idx_daily_reports_date ON daily_reports(date);
CREATE INDEX IF NOT EXISTS idx_daily_reports_unit_id ON daily_reports(unit_id);
CREATE UNIQUE INDEX IF NOT EXISTS idx_daily_reports_date_unit_unique ON daily_reports(date, COALESCE(unit_id, '__NULL__'));
CREATE INDEX IF NOT EXISTS idx_daily_report_meals_report_id ON daily_report_meals(daily_report_id);
CREATE INDEX IF NOT EXISTS idx_daily_report_meal_items_meal_id ON daily_report_meal_items(meal_id);
CREATE INDEX IF NOT EXISTS idx_daily_reports_fiscal_year ON daily_reports(fiscal_year);

-- Products & Stocks
CREATE INDEX IF NOT EXISTS idx_products_year ON products(year);
CREATE INDEX IF NOT EXISTS idx_products_name ON products(name);
CREATE INDEX IF NOT EXISTS idx_inventory_stocks_product_id ON inventory_stocks(product_id);

-- Units & Users
CREATE INDEX IF NOT EXISTS idx_units_wilaya_code ON units(wilaya_code);
CREATE INDEX IF NOT EXISTS idx_units_code ON units(code);
CREATE INDEX IF NOT EXISTS idx_users_username ON users(username);

-- Orders
CREATE INDEX IF NOT EXISTS idx_supplier_orders_order_date ON supplier_orders(order_date);
CREATE INDEX IF NOT EXISTS idx_supplier_orders_status ON supplier_orders(status);
CREATE INDEX IF NOT EXISTS idx_supplier_orders_fiscal_year ON supplier_orders(fiscal_year);
CREATE INDEX IF NOT EXISTS idx_daily_report_meal_items_meal_id ON daily_report_meal_items(meal_id);
CREATE INDEX IF NOT EXISTS idx_daily_report_meal_items_product_id ON daily_report_meal_items(product_id);
CREATE INDEX IF NOT EXISTS idx_supplier_order_items_order_id ON supplier_order_items(order_id);
CREATE INDEX IF NOT EXISTS idx_supplier_order_items_product_id ON supplier_order_items(product_id);

-- Audit
CREATE INDEX IF NOT EXISTS idx_audit_user_id ON audit_log(user_id);
CREATE INDEX IF NOT EXISTS idx_audit_timestamp ON audit_log(timestamp);
CREATE INDEX IF NOT EXISTS idx_audit_entity ON audit_log(entity_type, entity_id);
CREATE INDEX IF NOT EXISTS idx_audit_action ON audit_log(action);
CREATE INDEX IF NOT EXISTS idx_audit_status ON audit_log(status);
CREATE INDEX IF NOT EXISTS idx_audit_target_entity ON audit_log(target_type, target_id, timestamp);
CREATE INDEX IF NOT EXISTS idx_audit_actor_timestamp ON audit_log(actor_id, timestamp);
CREATE INDEX IF NOT EXISTS idx_audit_fiscal_event ON audit_log(fiscal_year, event_type);
CREATE INDEX IF NOT EXISTS idx_audit_keyset ON audit_log(timestamp, id);

-- Sync
CREATE INDEX IF NOT EXISTS idx_import_audit_events_package ON import_audit_events (package_id, occurred_at);
CREATE INDEX IF NOT EXISTS idx_import_audit_events_type_time ON import_audit_events (event_type, occurred_at);
CREATE INDEX IF NOT EXISTS idx_applied_sync_packages_imported_at ON applied_sync_packages (imported_at);
CREATE INDEX IF NOT EXISTS idx_sync_conflicts_unresolved ON sync_conflicts(resolved) WHERE resolved = 0;
CREATE INDEX IF NOT EXISTS idx_sync_conflicts_conflict_type ON sync_conflicts(conflict_type);
CREATE INDEX IF NOT EXISTS idx_sync_conflicts_source_node_id ON sync_conflicts(source_node_id);

-- Monthly Analytics
CREATE INDEX IF NOT EXISTS idx_monthly_reports_unit ON monthly_reports(unit_id);
CREATE INDEX IF NOT EXISTS idx_monthly_reports_year_month ON monthly_reports(report_year, report_month);
CREATE INDEX IF NOT EXISTS idx_monthly_reports_imported_at ON monthly_reports(imported_at);
CREATE INDEX IF NOT EXISTS idx_snapshots_unit_month ON unit_monthly_snapshots(unit_id, report_year, report_month);
CREATE INDEX IF NOT EXISTS idx_snapshots_balance_anomaly ON unit_monthly_snapshots(has_balance_anomaly) WHERE has_balance_anomaly = 1;
CREATE INDEX IF NOT EXISTS idx_snapshots_consumption_anomaly ON unit_monthly_snapshots(has_consumption_anomaly) WHERE has_consumption_anomaly = 1;
CREATE INDEX IF NOT EXISTS idx_snapshots_stale ON unit_monthly_snapshots(is_stale) WHERE is_stale = 1;

-- Movements
CREATE INDEX IF NOT EXISTS idx_stock_movements_product ON stock_movements(product_id);
CREATE INDEX IF NOT EXISTS idx_stock_movements_timestamp ON stock_movements(timestamp);
CREATE INDEX IF NOT EXISTS idx_stock_movements_type ON stock_movements(movement_type);
CREATE INDEX IF NOT EXISTS idx_stock_movements_reference ON stock_movements(reference_type, reference_id);
CREATE INDEX IF NOT EXISTS idx_stock_movements_unit_id ON stock_movements(unit_id);
CREATE INDEX IF NOT EXISTS idx_stock_movements_unit_timestamp ON stock_movements(unit_id, timestamp);
CREATE INDEX IF NOT EXISTS idx_stock_product_unit_time ON stock_movements(product_id, unit_id, timestamp);
CREATE INDEX IF NOT EXISTS idx_stock_unit_type_time ON stock_movements(unit_id, movement_type, timestamp);
CREATE INDEX IF NOT EXISTS idx_stock_in_partial ON stock_movements(unit_id, timestamp) WHERE movement_type='IN';
CREATE INDEX IF NOT EXISTS idx_stock_movements_fiscal_year ON stock_movements(fiscal_year);
CREATE INDEX IF NOT EXISTS idx_fifo_active ON fifo_stock_layers(unit_id, product_id, received_at ASC, id ASC) WHERE qty_remaining > 0;
CREATE INDEX IF NOT EXISTS idx_fifo_source ON fifo_stock_layers(source_type, source_id);
CREATE INDEX IF NOT EXISTS idx_layer_consumptions_layer ON inventory_layer_consumptions(layer_id);
CREATE INDEX IF NOT EXISTS idx_layer_consumptions_movement ON inventory_layer_consumptions(movement_id);

-- Snapshots
CREATE INDEX IF NOT EXISTS idx_opening_balance_snapshots_product ON opening_balance_snapshots(product_id);
CREATE INDEX IF NOT EXISTS idx_opening_balance_snapshots_fiscal_year ON opening_balance_snapshots(fiscal_year);
CREATE INDEX IF NOT EXISTS idx_report_generation_metadata_year ON report_generation_metadata(fiscal_year);
CREATE INDEX IF NOT EXISTS idx_ref_price_year ON reference_price_snapshots(fiscal_year);

-- Sync Support
CREATE INDEX IF NOT EXISTS idx_products_sync ON products(updated_at, node_id, deleted) WHERE deleted = 0;
CREATE INDEX IF NOT EXISTS idx_users_sync ON users(updated_at, node_id, deleted) WHERE deleted = 0;
CREATE INDEX IF NOT EXISTS idx_units_sync ON units(updated_at, node_id, deleted) WHERE deleted = 0;
CREATE INDEX IF NOT EXISTS idx_inventory_stocks_sync ON inventory_stocks(updated_at, node_id, deleted) WHERE deleted = 0;
CREATE INDEX IF NOT EXISTS idx_daily_reports_sync ON daily_reports(updated_at, node_id, deleted) WHERE deleted = 0;
CREATE INDEX IF NOT EXISTS idx_supplier_orders_sync ON supplier_orders(updated_at, node_id, deleted) WHERE deleted = 0;
CREATE INDEX IF NOT EXISTS idx_stock_movements_sync ON stock_movements(updated_at, deleted) WHERE deleted = 0;

-- Observability Support
CREATE INDEX IF NOT EXISTS idx_fiscal_op_snapshots_year ON fiscal_operational_snapshots(fiscal_year);
CREATE INDEX IF NOT EXISTS idx_fiscal_op_snapshots_date ON fiscal_operational_snapshots(snapshot_date);
CREATE INDEX IF NOT EXISTS idx_integrity_attempts_outcome ON integrity_verification_attempts(outcome, attempted_at);
CREATE INDEX IF NOT EXISTS idx_integrity_attempts_type ON integrity_verification_attempts(verification_type, attempted_at);
CREATE INDEX IF NOT EXISTS idx_op_findings_emitted_at ON operational_findings_log(emitted_at);
CREATE INDEX IF NOT EXISTS idx_op_findings_severity ON operational_findings_log(severity);
CREATE INDEX IF NOT EXISTS idx_operational_sessions_started ON operational_sessions(started_at);
CREATE INDEX IF NOT EXISTS idx_operational_sessions_open ON operational_sessions(ended_at) WHERE ended_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_import_repro_metadata_package ON import_reproducibility_metadata(package_id, imported_at);
CREATE INDEX IF NOT EXISTS idx_applied_fiscal_transitions_id ON applied_fiscal_transitions(fiscal_transition_id);
CREATE INDEX IF NOT EXISTS idx_fiscal_package_registry_transition_id ON fiscal_closure_package_registry(transition_id);
CREATE INDEX IF NOT EXISTS idx_fiscal_package_registry_fiscal_year ON fiscal_closure_package_registry(fiscal_year);
CREATE INDEX IF NOT EXISTS idx_fiscal_package_registry_exported_at ON fiscal_closure_package_registry(exported_at);
CREATE INDEX IF NOT EXISTS idx_telemetry_timestamp ON telemetry_events(timestamp);
CREATE INDEX IF NOT EXISTS idx_telemetry_event_type ON telemetry_events(event_type);

-- Identity Store (consolidated from migrations 002/003/008)
-- Invariant 6: exactly one ACTIVE credential per subject (WILAYA singleton included).
CREATE UNIQUE INDEX IF NOT EXISTS idx_identity_active_subject
    ON identity_store(subject_type, subject_id) WHERE status = 'ACTIVE' AND deleted = 0;

-- Single ACTIVE ADMIN (SEC-002): at most one ACTIVE ADMIN credential.
CREATE UNIQUE INDEX IF NOT EXISTS idx_identity_single_active_admin
    ON identity_store(subject_type)
    WHERE subject_type = 'ADMIN' AND status = 'ACTIVE' AND deleted = 0;

-- Invariants 2/3: credential lifecycle traversal (credential_id, generation).
CREATE INDEX IF NOT EXISTS idx_identity_credential_generation
    ON identity_store(credential_id, generation);

-- Trust chain traversal (issuer -> issued); NULL issuer = offline Authority Root.
CREATE INDEX IF NOT EXISTS idx_identity_issuer
    ON identity_store(issuer_identity_id);

-- Local node resolution (singleton subject types, e.g. WILAYA).
CREATE INDEX IF NOT EXISTS idx_identity_subject_type_status
    ON identity_store(subject_type, status, generation);

-- =============================================================================
-- 9. TRIGGERS (SYNC & TEMPORAL INTEGRITY)
-- =============================================================================

CREATE TRIGGER IF NOT EXISTS trg_products_updated_at AFTER UPDATE ON products FOR EACH ROW WHEN NEW.updated_at IS OLD.updated_at OR NEW.updated_at IS NULL BEGIN UPDATE products SET updated_at = datetime('now') WHERE id = NEW.id; END;
CREATE TRIGGER IF NOT EXISTS trg_users_updated_at AFTER UPDATE ON users FOR EACH ROW WHEN NEW.updated_at IS OLD.updated_at OR NEW.updated_at IS NULL BEGIN UPDATE users SET updated_at = datetime('now') WHERE id = NEW.id; END;
CREATE TRIGGER IF NOT EXISTS trg_units_updated_at AFTER UPDATE ON units FOR EACH ROW WHEN NEW.updated_at IS OLD.updated_at OR NEW.updated_at IS NULL BEGIN UPDATE units SET updated_at = datetime('now') WHERE id = NEW.id; END;
CREATE TRIGGER IF NOT EXISTS trg_inventory_stocks_updated_at AFTER UPDATE ON inventory_stocks FOR EACH ROW WHEN NEW.updated_at IS OLD.updated_at OR NEW.updated_at IS NULL BEGIN UPDATE inventory_stocks SET updated_at = datetime('now') WHERE id = NEW.id; END;
CREATE TRIGGER IF NOT EXISTS trg_daily_reports_updated_at AFTER UPDATE ON daily_reports FOR EACH ROW WHEN NEW.updated_at IS OLD.updated_at OR NEW.updated_at IS NULL BEGIN UPDATE daily_reports SET updated_at = datetime('now') WHERE id = NEW.id; END;
CREATE TRIGGER IF NOT EXISTS trg_supplier_orders_updated_at AFTER UPDATE ON supplier_orders FOR EACH ROW WHEN NEW.updated_at IS OLD.updated_at OR NEW.updated_at IS NULL BEGIN UPDATE supplier_orders SET updated_at = datetime('now') WHERE id = NEW.id; END;
CREATE TRIGGER IF NOT EXISTS trg_stock_movements_updated_at AFTER UPDATE ON stock_movements FOR EACH ROW WHEN NEW.updated_at IS OLD.updated_at OR NEW.updated_at IS NULL BEGIN UPDATE stock_movements SET updated_at = datetime('now') WHERE id = NEW.id; END;

-- =============================================================================
-- 10. BASELINE SEED DATA
-- =============================================================================

INSERT OR IGNORE INTO settings (id, node_type, current_year, configured) VALUES (1, 'UNCONFIGURED', 2026, 0);
INSERT OR IGNORE INTO users (id, username, password_hash, role, created_at, node_id) VALUES ('system', 'system', 'disabled', 'System', datetime('now'), 'system');
INSERT OR IGNORE INTO fiscal_year_status (year, status, opened_at) VALUES (CAST(strftime('%Y', 'now') AS INTEGER), 'open', strftime('%Y-%m-%dT%H:%M:%SZ', 'now'));

-- =============================================================================
-- 11. DOMAIN EVENTS (MIGRATION 003)
-- =============================================================================

CREATE TABLE IF NOT EXISTS domain_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    transaction_id TEXT NOT NULL,
    sequence_number INTEGER NOT NULL,
    category TEXT NOT NULL,
    event_type TEXT NOT NULL,
    event_body TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    UNIQUE(transaction_id, sequence_number)
);

CREATE INDEX IF NOT EXISTS idx_domain_events_replay
    ON domain_events(transaction_id, sequence_number);

CREATE INDEX IF NOT EXISTS idx_domain_events_category
    ON domain_events(category, transaction_id, sequence_number);

CREATE INDEX IF NOT EXISTS idx_domain_events_created_at
    ON domain_events(created_at);

-- =============================================================================
-- 12. RATE LIMITER (MIGRATION 005)
-- =============================================================================

CREATE TABLE IF NOT EXISTS rate_limiter_attempts (
    key TEXT PRIMARY KEY,
    count INTEGER NOT NULL DEFAULT 0,
    successful_count INTEGER NOT NULL DEFAULT 0,
    total_failed_count INTEGER NOT NULL DEFAULT 0,
    first_attempt_at INTEGER NOT NULL,
    last_attempt_at INTEGER NOT NULL
);


