//! Comprehensive tests for Phase 4 — Audit Schema Evolution.
//!
//! Tests cover:
//! - Dual-write compatibility (structured columns + details JSON)
//! - Legacy row compatibility (flat columns only)
//! - Mixed-schema reads (old + new rows)
//! - Deterministic ordering (timestamp + id tiebreaker)
//! - Keyset pagination correctness
//! - Reconstruction stability
//! - Serialization reproducibility
//! - Backward compatibility (old OFFSET-based APIs still work)

use grpc_lib::db::ConnectionFactory;
use grpc_lib::domain::audit::{
    to_audit_event, AuditEvent, AuditEventType, AuditQuery, NewAuditEntry,
};
use grpc_lib::domain::audit_chain::compute_entry_hash;
use grpc_lib::repositories::executor::DbExecutor;
use grpc_lib::repositories::RepositoryProvider;
use tempfile::tempdir;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn admin_id(conn: &rusqlite::Connection) -> String {
    conn.query_row(
        "SELECT id FROM users WHERE username = 'admin' LIMIT 1",
        [],
        |row| row.get::<_, String>(0),
    )
    .expect("admin user")
}

/// Build a NewAuditEntry with the full set of structured fields (simulating new service).
fn new_style_entry(
    id: &str,
    ts: &str,
    uid: &str,
    action: &str,
    entity: &str,
) -> NewAuditEntry {
    NewAuditEntry {
        id: id.to_string(),
        user_id: uid.to_string(),
        username: "operator".into(),
        action: action.to_string(),
        entity_type: entity.to_string(),
        entity_id: Some("target-123".into()),
        entity_name: Some("Target Entity".into()),
        old_value: Some(r#"{"qty":5}"#.into()),
        new_value: Some(r#"{"qty":10}"#.into()),
        session_id: Some("sess-abc".into()),
        timestamp: ts.to_string(),
        status: "Success".into(),
        error_message: None,
        metadata: Some(r#"{"meta":"data"}"#.into()),
        previous_hash: None,
        entry_hash: None,
        event_type: Some("UserAction".into()),
        actor_id: Some(uid.to_string()),
        target_type: Some(entity.to_string()),
        target_id: Some("target-123".into()),
        fiscal_year: Some(2025),
        before_snapshot: Some(r#"{"qty":5}"#.into()),
        after_snapshot: Some(r#"{"qty":10}"#.into()),
        node_id: Some("node-42".into()),
        details: None,
    }
}

fn build_test_details(entry: &NewAuditEntry) -> String {
    serde_json::json!({
        "id": entry.id,
        "action": entry.action,
        "entity_type": entry.entity_type,
        "entity_id": entry.entity_id,
        "entity_name": entry.entity_name,
        "username": entry.username,
        "old_value": entry.old_value,
        "new_value": entry.new_value,
        "session_id": entry.session_id,
        "status": entry.status,
        "error_message": entry.error_message,
        "metadata": entry.metadata,
    }).to_string()
}

/// Build a NewAuditEntry with ONLY legacy flat columns (simulating old code path).
fn legacy_style_entry(id: &str, ts: &str, uid: &str, action: &str, entity: &str) -> NewAuditEntry {
    NewAuditEntry {
        id: id.to_string(),
        user_id: uid.to_string(),
        username: "legacy_user".into(),
        action: action.to_string(),
        entity_type: entity.to_string(),
        entity_id: None,
        entity_name: None,
        old_value: None,
        new_value: None,
        session_id: None,
        timestamp: ts.to_string(),
        status: "Success".into(),
        error_message: None,
        metadata: None,
        previous_hash: None,
        entry_hash: None,
        event_type: None,
        actor_id: None,
        target_type: None,
        target_id: None,
        fiscal_year: None,
        before_snapshot: None,
        after_snapshot: None,
        node_id: None,
        details: None,
    }
}

fn insert_entry(conn: &rusqlite::Connection, entry: NewAuditEntry) {
    // Populate details JSON if not already set (dual-write contract)
    let details = entry.details.clone().or_else(|| {
        if entry.event_type.is_some() {
            Some(build_test_details(&entry))
        } else {
            None
        }
    });
    let ex = DbExecutor::Conn(conn);
    let repo = ex.audit();
    let prev = repo.fetch_latest_entry_hash().unwrap();
    let digest = compute_entry_hash(prev.as_deref(), &entry);
    let chained = NewAuditEntry {
        previous_hash: prev,
        entry_hash: Some(digest),
        details,
        ..entry
    };
    repo.insert_audit_log(&chained).unwrap();
}

// ---------------------------------------------------------------------------
// Test: Dual-write compatibility
// ---------------------------------------------------------------------------

#[test]
fn dual_write_populates_structured_columns() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("dual_write.db");
    let db = ConnectionFactory::new_with_path(&path).expect("db");
    let conn = db.get_connection();
    let uid = admin_id(conn);

    let entry = new_style_entry("dw-1", "2026-06-01T10:00:00Z", &uid, "Login", "User");
    insert_entry(conn, entry);

    // Read back via projected event row
    let ex = DbExecutor::Conn(conn);
    let rows = ex.audit().fetch_event_rows(&AuditQuery {
        user_id: None,
        action: None,
        entity_type: None,
        start_timestamp: None,
        end_timestamp: None,
        status: None,
        search_like: None,
        limit: 100,
        offset: 0,
    }).unwrap();

    assert_eq!(rows.len(), 1, "expected 1 row");
    let row = &rows[0];
    assert_eq!(row.event_type.as_deref(), Some("UserAction"), "dual-write: event_type");
    assert_eq!(row.actor_id.as_deref(), Some(uid.as_str()), "dual-write: actor_id");
    assert_eq!(row.fiscal_year, Some(2025), "dual-write: fiscal_year");
    assert_eq!(row.node_id.as_deref(), Some("node-42"), "dual-write: node_id");
    assert!(row.details_json.is_some(), "dual-write: details must be populated");

    // Convert to AuditEvent and verify
    let event = to_audit_event(row.clone()).expect("to_audit_event");
    assert_eq!(event.event_type, AuditEventType::UserAction);
    assert_eq!(event.actor_id.as_deref(), Some(uid.as_str()));
    assert_eq!(event.fiscal_year, Some(2025));
    assert_eq!(event.node_id.as_deref(), Some("node-42"));
    assert_eq!(event.target_id.as_deref(), Some("target-123"));
    assert_eq!(event.before_snapshot, Some(serde_json::json!({"qty":5})));
    assert_eq!(event.after_snapshot, Some(serde_json::json!({"qty":10})));
}

#[test]
fn dual_write_details_json_is_valid() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("dual_write_details.db");
    let db = ConnectionFactory::new_with_path(&path).expect("db");
    let conn = db.get_connection();
    let uid = admin_id(conn);

    let entry = new_style_entry("dw-2", "2026-06-01T11:00:00Z", &uid, "CreateUnit", "Unit");
    insert_entry(conn, entry);

    let ex = DbExecutor::Conn(conn);
    let rows = ex.audit().fetch_event_rows(&AuditQuery {
        user_id: None,
        action: None,
        entity_type: None,
        start_timestamp: None,
        end_timestamp: None,
        status: None,
        search_like: None,
        limit: 100,
        offset: 0,
    }).unwrap();

    let row = &rows[0];
    let details: serde_json::Value = serde_json::from_str(row.details_json.as_ref().unwrap()).unwrap();
    assert_eq!(details["action"], "CreateUnit");
    assert_eq!(details["entity_type"], "Unit");
    assert_eq!(details["entity_id"], "target-123");
    assert_eq!(details["username"], "operator");
    assert_eq!(details["status"], "Success");
}

// ---------------------------------------------------------------------------
// Test: Legacy row compatibility
// ---------------------------------------------------------------------------

#[test]
fn legacy_row_reconstructs_from_flat_columns() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("legacy_reconstruct.db");
    let db = ConnectionFactory::new_with_path(&path).expect("db");
    let conn = db.get_connection();
    let uid = admin_id(conn);

    let entry = legacy_style_entry("leg-1", "2026-05-15T08:00:00Z", &uid, "Login", "User");
    insert_entry(conn, entry);

    let ex = DbExecutor::Conn(conn);
    let rows = ex.audit().fetch_event_rows(&AuditQuery {
        user_id: None,
        action: None,
        entity_type: None,
        start_timestamp: None,
        end_timestamp: None,
        status: None,
        search_like: None,
        limit: 100,
        offset: 0,
    }).unwrap();

    let row = &rows[0];
    // Legacy rows have NULL structured columns
    assert!(row.event_type.is_none(), "legacy: event_type is null");
    assert!(row.details_json.is_none(), "legacy: details is null");

    let event = to_audit_event(row.clone()).expect("to_audit_event from legacy");
    assert_eq!(event.event_type, AuditEventType::UserAction);
    assert_eq!(event.actor_id.as_deref(), Some(uid.as_str()));
    assert_eq!(event.actor_name.as_deref(), Some("legacy_user"));
    assert_eq!(event.target_type.as_deref(), Some("User"));
    assert!(event.fiscal_year.is_none(), "legacy: no fiscal_year");
    assert!(event.node_id.is_none(), "legacy: no node_id");
}

#[test]
fn legacy_fiscal_action_has_correct_event_type() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("legacy_fiscal.db");
    let db = ConnectionFactory::new_with_path(&path).expect("db");
    let conn = db.get_connection();
    let uid = admin_id(conn);

    let entry = legacy_style_entry("leg-2", "2026-04-01T00:00:00Z", &uid, "FiscalYearClosed", "Financial");
    insert_entry(conn, entry);

    let ex = DbExecutor::Conn(conn);
    let rows = ex.audit().fetch_event_rows(&AuditQuery {
        user_id: None,
        action: None,
        entity_type: None,
        start_timestamp: None,
        end_timestamp: None,
        status: None,
        search_like: None,
        limit: 100,
        offset: 0,
    }).unwrap();

    let event = to_audit_event(rows[0].clone()).expect("to_audit_event");
    assert_eq!(event.event_type, AuditEventType::FiscalEvent);
}

#[test]
fn legacy_sync_action_has_correct_event_type() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("legacy_sync.db");
    let db = ConnectionFactory::new_with_path(&path).expect("db");
    let conn = db.get_connection();
    let uid = admin_id(conn);

    let entry = legacy_style_entry("leg-3", "2026-03-01T12:00:00Z", &uid, "ImportProducts", "Product");
    insert_entry(conn, entry);

    let ex = DbExecutor::Conn(conn);
    let rows = ex.audit().fetch_event_rows(&AuditQuery {
        user_id: None,
        action: None,
        entity_type: None,
        start_timestamp: None,
        end_timestamp: None,
        status: None,
        search_like: None,
        limit: 100,
        offset: 0,
    }).unwrap();

    let event = to_audit_event(rows[0].clone()).expect("to_audit_event");
    assert_eq!(event.event_type, AuditEventType::SyncEvent);
}

#[test]
fn legacy_integrity_action_has_correct_event_type() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("legacy_integrity.db");
    let db = ConnectionFactory::new_with_path(&path).expect("db");
    let conn = db.get_connection();
    let uid = admin_id(conn);

    let entry = legacy_style_entry(
        "leg-4",
        "2026-02-01T00:00:00Z",
        &uid,
        "BackupCheckpointFailed",
        "System",
    );
    insert_entry(conn, entry);

    let ex = DbExecutor::Conn(conn);
    let rows = ex.audit().fetch_event_rows(&AuditQuery {
        user_id: None,
        action: None,
        entity_type: None,
        start_timestamp: None,
        end_timestamp: None,
        status: None,
        search_like: None,
        limit: 100,
        offset: 0,
    }).unwrap();

    let event = to_audit_event(rows[0].clone()).expect("to_audit_event");
    assert_eq!(event.event_type, AuditEventType::IntegrityEvent);
}

// ---------------------------------------------------------------------------
// Test: Mixed-schema reads
// ---------------------------------------------------------------------------

#[test]
fn mixed_schema_reads_old_and_new_rows() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("mixed_schema.db");
    let db = ConnectionFactory::new_with_path(&path).expect("db");
    let conn = db.get_connection();
    let uid = admin_id(conn);

    // Insert new-style entry (with structured columns)
    let new_entry = new_style_entry("mix-new", "2026-06-01T12:00:00Z", &uid, "Login", "User");
    insert_entry(conn, new_entry);

    // Insert legacy-style entry (flat columns only)
    let old_entry = legacy_style_entry("mix-old", "2026-06-01T11:00:00Z", &uid, "Login", "User");
    insert_entry(conn, old_entry);

    let ex = DbExecutor::Conn(conn);
    let rows = ex.audit().fetch_event_rows(&AuditQuery {
        user_id: None,
        action: None,
        entity_type: None,
        start_timestamp: None,
        end_timestamp: None,
        status: None,
        search_like: None,
        limit: 100,
        offset: 0,
    }).unwrap();

    assert_eq!(rows.len(), 2, "mixed: both rows returned");

    // Both should convert to AuditEvent
    for row in &rows {
        let event = to_audit_event(row.clone()).expect("mixed: to_audit_event");
        assert_eq!(event.event_type, AuditEventType::UserAction);
    }

    // New-style row has fiscal_year
    let new_event = to_audit_event(rows[0].clone()).unwrap();
    let old_event = to_audit_event(rows[1].clone()).unwrap();
    // Newer timestamp sorts first (DESC order from fetch_event_rows)
    assert_eq!(new_event.id, "mix-new");
    assert_eq!(new_event.fiscal_year, Some(2025));
    assert_eq!(old_event.id, "mix-old");
    assert!(old_event.fiscal_year.is_none());
}

// ---------------------------------------------------------------------------
// Test: Deterministic ordering
// ---------------------------------------------------------------------------

#[test]
fn keyset_ordering_is_deterministic() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("keyset_order.db");
    let db = ConnectionFactory::new_with_path(&path).expect("db");
    let conn = db.get_connection();
    let uid = admin_id(conn);

    // Insert rows with same timestamp to test tiebreaker
    let ids = ["k-a", "k-b", "k-c"];
    for id in ids.iter() {
        let mut entry = legacy_style_entry(id, "2026-06-01T10:00:00Z", &uid, "Login", "User");
        // Force deterministic ids for ordering
        entry.id = id.to_string();
        insert_entry(conn, entry);
    }

    // Read via keyset (ASC order to verify tiebreaker)
    let ex = DbExecutor::Conn(conn);
    let rows = ex.audit().fetch_event_rows_keyset(
        &AuditQuery {
            user_id: None,
            action: None,
            entity_type: None,
            start_timestamp: None,
            end_timestamp: None,
            status: None,
            search_like: None,
            limit: 100,
            offset: 0,
        },
        None,
        None,
    ).unwrap();

    assert_eq!(rows.len(), 3, "keyset: all rows returned");
    // With same timestamp, tiebreaker is id ASC → k-a, k-b, k-c
    assert_eq!(rows[0].id, "k-a", "keyset order: first");
    assert_eq!(rows[1].id, "k-b", "keyset order: second");
    assert_eq!(rows[2].id, "k-c", "keyset order: third");

    // Second read must produce identical order
    let rows2 = ex.audit().fetch_event_rows_keyset(
        &AuditQuery {
            user_id: None,
            action: None,
            entity_type: None,
            start_timestamp: None,
            end_timestamp: None,
            status: None,
            search_like: None,
            limit: 100,
            offset: 0,
        },
        None,
        None,
    ).unwrap();

    for (i, row) in rows2.iter().enumerate() {
        assert_eq!(row.id, rows[i].id, "keyset: reproducible at index {i}");
    }
}

// ---------------------------------------------------------------------------
// Test: Keyset pagination correctness
// ---------------------------------------------------------------------------

#[test]
fn keyset_pagination_returns_contiguous_pages() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("keyset_pages.db");
    let db = ConnectionFactory::new_with_path(&path).expect("db");
    let conn = db.get_connection();
    let uid = admin_id(conn);

    // Insert 5 rows with increasing timestamps
    for i in 0..5 {
        let ts = format!("2026-06-{:02}T10:00:00Z", i + 1);
        let id = format!("kp-{}", i);
        let entry = legacy_style_entry(&id, &ts, &uid, "Login", "User");
        insert_entry(conn, entry);
    }

    let ex = DbExecutor::Conn(conn);

    // Page 1: limit=2, no keyset → first 2 rows
    let page1 = ex.audit().fetch_event_rows_keyset(
        &AuditQuery {
            user_id: None,
            action: None,
            entity_type: None,
            start_timestamp: None,
            end_timestamp: None,
            status: None,
            search_like: None,
            limit: 2,
            offset: 0,
        },
        None,
        None,
    ).unwrap();
    assert_eq!(page1.len(), 2, "page1: 2 rows");
    assert_eq!(page1[0].id, "kp-0", "page1: first row");
    assert_eq!(page1[1].id, "kp-1", "page1: second row");

    // Page 2: keyset from last row of page1
    let last = &page1[1];
    let page2 = ex.audit().fetch_event_rows_keyset(
        &AuditQuery {
            user_id: None,
            action: None,
            entity_type: None,
            start_timestamp: None,
            end_timestamp: None,
            status: None,
            search_like: None,
            limit: 2,
            offset: 0,
        },
        Some(&last.timestamp),
        Some(&last.id),
    ).unwrap();
    assert_eq!(page2.len(), 2, "page2: 2 rows");
    assert_eq!(page2[0].id, "kp-2", "page2: first row");
    assert_eq!(page2[1].id, "kp-3", "page2: second row");

    // Page 3: keyset from last row of page2
    let last2 = &page2[1];
    let page3 = ex.audit().fetch_event_rows_keyset(
        &AuditQuery {
            user_id: None,
            action: None,
            entity_type: None,
            start_timestamp: None,
            end_timestamp: None,
            status: None,
            search_like: None,
            limit: 2,
            offset: 0,
        },
        Some(&last2.timestamp),
        Some(&last2.id),
    ).unwrap();
    assert_eq!(page3.len(), 1, "page3: 1 row (last)");
    assert_eq!(page3[0].id, "kp-4", "page3: last row");

    // No more rows
    let last3 = &page3[0];
    let page4 = ex.audit().fetch_event_rows_keyset(
        &AuditQuery {
            user_id: None,
            action: None,
            entity_type: None,
            start_timestamp: None,
            end_timestamp: None,
            status: None,
            search_like: None,
            limit: 2,
            offset: 0,
        },
        Some(&last3.timestamp),
        Some(&last3.id),
    ).unwrap();
    assert!(page4.is_empty(), "page4: no more rows");
}

// ---------------------------------------------------------------------------
// Test: Reconstruction stability (deterministic to_audit_event)
// ---------------------------------------------------------------------------

#[test]
fn to_audit_event_is_stable_across_calls() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("reconstruction_stable.db");
    let db = ConnectionFactory::new_with_path(&path).expect("db");
    let conn = db.get_connection();
    let uid = admin_id(conn);

    let entry = new_style_entry("stable-1", "2026-07-01T10:00:00Z", &uid, "FiscalYearClosed", "Financial");
    insert_entry(conn, entry);

    let ex = DbExecutor::Conn(conn);
    let rows = ex.audit().fetch_event_rows(&AuditQuery {
        user_id: None,
        action: None,
        entity_type: None,
        start_timestamp: None,
        end_timestamp: None,
        status: None,
        search_like: None,
        limit: 100,
        offset: 0,
    }).unwrap();

    // Call to_audit_event twice
    let event_a = to_audit_event(rows[0].clone()).unwrap();
    let event_b = to_audit_event(rows[0].clone()).unwrap();

    // Serialize both and compare
    let json_a = serde_json::to_string(&event_a).unwrap();
    let json_b = serde_json::to_string(&event_b).unwrap();
    assert_eq!(json_a, json_b, "reconstruction must be stable across calls");
}

#[test]
fn legacy_reconstruction_is_stable() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("legacy_stable.db");
    let db = ConnectionFactory::new_with_path(&path).expect("db");
    let conn = db.get_connection();
    let uid = admin_id(conn);

    let entry = legacy_style_entry("legacy-stable", "2026-05-01T10:00:00Z", &uid, "Login", "User");
    insert_entry(conn, entry);

    let ex = DbExecutor::Conn(conn);
    let rows = ex.audit().fetch_event_rows(&AuditQuery {
        user_id: None,
        action: None,
        entity_type: None,
        start_timestamp: None,
        end_timestamp: None,
        status: None,
        search_like: None,
        limit: 100,
        offset: 0,
    }).unwrap();

    let event_a = to_audit_event(rows[0].clone()).unwrap();
    let event_b = to_audit_event(rows[0].clone()).unwrap();
    assert_eq!(
        serde_json::to_string(&event_a).unwrap(),
        serde_json::to_string(&event_b).unwrap(),
        "legacy reconstruction must be stable"
    );
}

// ---------------------------------------------------------------------------
// Test: Serialization reproducibility
// ---------------------------------------------------------------------------

#[test]
fn audit_event_serialization_round_trip() {
    let event = AuditEvent {
        id: "ser-1".into(),
        event_type: AuditEventType::FiscalEvent,
        actor_id: Some("actor-1".into()),
        actor_name: Some("Alice".into()),
        target_type: Some("Financial".into()),
        target_id: Some("fy-2025".into()),
        fiscal_year: Some(2025),
        before_snapshot: Some(serde_json::json!({"balance": 1000.0})),
        after_snapshot: Some(serde_json::json!({"balance": 0.0})),
        node_id: None,
        details: serde_json::json!({"action": "FiscalYearClosed", "entity_type": "Financial"}),
        action: "FiscalYearClosed".into(),
        action_display_arabic: "إغلاق سنة مالية".into(),
        entity_type: "Financial".into(),
        entity_type_display_arabic: "مالي".into(),
        entity_name: Some("FY 2025".into()),
        session_id: None,
        timestamp: "2026-07-01T10:00:00Z".into(),
        status: "Success".into(),
        error_message: None,
        previous_hash: Some("abc".into()),
        entry_hash: Some("def".into()),
    };

    let json = serde_json::to_string(&event).unwrap();
    let deserialized: AuditEvent = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.id, event.id);
    assert_eq!(deserialized.event_type, event.event_type);
    assert_eq!(deserialized.fiscal_year, event.fiscal_year);
    assert_eq!(deserialized.before_snapshot, event.before_snapshot);
    assert_eq!(deserialized.details, event.details);
    assert_eq!(deserialized.action_display_arabic, event.action_display_arabic);
    assert_eq!(deserialized.entry_hash, event.entry_hash);
}

// ---------------------------------------------------------------------------
// Test: Backward compatibility (legacy OFFSET APIs still work)
// ---------------------------------------------------------------------------

#[test]
fn legacy_fetch_entries_still_works() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("backward_compat.db");
    let db = ConnectionFactory::new_with_path(&path).expect("db");
    let conn = db.get_connection();
    let uid = admin_id(conn);

    // Insert both legacy and new-style entries
    let entry1 = legacy_style_entry("bc-1", "2026-06-01T10:00:00Z", &uid, "Login", "User");
    insert_entry(conn, entry1);
    let entry2 = new_style_entry("bc-2", "2026-06-02T10:00:00Z", &uid, "CreateUnit", "Unit");
    insert_entry(conn, entry2);

    // Legacy fetch_entries should still work (returns AuditEntryDbRow)
    let q = AuditQuery {
        user_id: None,
        action: None,
        entity_type: None,
        start_timestamp: None,
        end_timestamp: None,
        status: None,
        search_like: None,
        limit: 100,
        offset: 0,
    };
    let ex = DbExecutor::Conn(conn);
    let legacy_rows = ex.audit().fetch_entries(&q).unwrap();
    assert_eq!(legacy_rows.len(), 2, "legacy fetch: 2 rows");
    assert_eq!(legacy_rows[0].action, "CreateUnit", "legacy: newest first");
    assert_eq!(legacy_rows[1].action, "Login", "legacy: oldest second");
}

// ---------------------------------------------------------------------------
// Test: Chain verification still works (not affected by new columns)
// ---------------------------------------------------------------------------

#[test]
fn chain_verification_works_with_dual_write_entries() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("chain_dual_write.db");
    let db = ConnectionFactory::new_with_path(&path).expect("db");
    let conn = db.get_connection();
    let uid = admin_id(conn);

    // Insert two new-style entries (chained)
    let entry1 = new_style_entry("chain-1", "2026-08-01T10:00:00Z", &uid, "Login", "User");
    insert_entry(conn, entry1);
    let entry2 = new_style_entry("chain-2", "2026-08-01T10:01:00Z", &uid, "Logout", "User");
    insert_entry(conn, entry2);

    // Chain verification must pass
    let ex = DbExecutor::Conn(conn);
    let summary = ex.audit().verify_audit_chain_streaming().unwrap();
    assert!(summary.is_valid, "chain verification must pass with dual-write entries");
    assert_eq!(summary.verified_entries, 2, "both entries verified");
}

#[test]
fn chain_verification_works_with_mixed_entries() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("chain_mixed.db");
    let db = ConnectionFactory::new_with_path(&path).expect("db");
    let conn = db.get_connection();
    let uid = admin_id(conn);

    // Mix of legacy and new-style entries
    let e1 = legacy_style_entry("mix-chain-1", "2026-08-01T10:00:00Z", &uid, "Login", "User");
    let ex = DbExecutor::Conn(conn);
    let repo = ex.audit();
    let h1 = compute_entry_hash(None, &e1);
    repo.insert_audit_log(&NewAuditEntry {
        previous_hash: None,
        entry_hash: Some(h1.clone()),
        ..e1
    }).unwrap();

    let e2 = new_style_entry("mix-chain-2", "2026-08-01T10:01:00Z", &uid, "Logout", "User");
    let h2 = compute_entry_hash(Some(&h1), &e2);
    repo.insert_audit_log(&NewAuditEntry {
        previous_hash: Some(h1),
        entry_hash: Some(h2),
        ..e2
    }).unwrap();

    let summary = repo.verify_audit_chain_streaming().unwrap();
    assert!(summary.is_valid, "mixed chain verification must pass");
    assert_eq!(summary.verified_entries, 2);
}
