use grpc_lib::domain::audit::NewAuditEntry;
use grpc_lib::domain::audit_chain::compute_entry_hash;
use grpc_lib::domain::events::{
    validate_replay_ordering, validate_transaction_sequences, DomainEvent, EventBuffer, StoredEvent,
};
use grpc_lib::domain::fifo_engine::simulate_fifo_consumption;
use grpc_lib::domain::invariants::audit::check_audit_chain_integrity;
use grpc_lib::domain::invariants::audit::ChainEntry;
use grpc_lib::domain::invariants::fifo::check_fifo_consumption_order;
use grpc_lib::domain::invariants::fifo::{LayerAtConsumption, PreConsumptionLayer};
use grpc_lib::domain::invariants::fiscal::check_origin_immutable;
use grpc_lib::domain::invariants::fiscal::check_single_open_fiscal_year;
use grpc_lib::domain::invariants::fiscal::{FiscalYearEntry, OriginYearRecord};
use grpc_lib::domain::invariants::stock::check_layer_quantity;
use grpc_lib::domain::invariants::stock::check_product_balance;
use grpc_lib::domain::invariants::stock::check_stock_non_negative;
use grpc_lib::domain::invariants::stock::{LayerQuantity, ProductBalance};
use grpc_lib::domain::meal_cost_engine::compute_meal_fifo_costs;
use grpc_lib::errors::AppError;
use grpc_lib::infrastructure::sync::packages::canonical_json::canonical_json_bytes;
use grpc_lib::models::{ConsumptionItemInput, MealSectionInput, MealType};
use serde_json::json;
use std::collections::HashMap;
use uuid::Uuid;

fn consume_from_map(
    map: &mut HashMap<String, Vec<(String, f64, f64)>>,
    product_id: &str,
    quantity: f64,
) -> Result<Vec<grpc_lib::domain::accounting::fifo::ConsumedLayerPortion>, AppError> {
    let layers = map
        .get(product_id)
        .ok_or_else(|| AppError::Internal(format!("unknown product: {}", product_id)))?;
    simulate_fifo_consumption(product_id, layers, quantity)
}

fn json_eq<T: serde::Serialize>(a: &T, b: &T) -> bool {
    serde_json::to_string(a).unwrap() == serde_json::to_string(b).unwrap()
}

#[test]
fn simulate_fifo_consumption_determinism() {
    let product_id = "test-product";
    let layers = vec![
        ("layer-1".into(), 400.0, 50.0),
        ("layer-2".into(), 500.0, 30.0),
    ];
    let quantity = 60.0;

    let r1 = simulate_fifo_consumption(product_id, &layers, quantity).unwrap();
    let r2 = simulate_fifo_consumption(product_id, &layers, quantity).unwrap();

    assert!(
        json_eq(&r1, &r2),
        "simulate_fifo_consumption must be deterministic"
    );
    assert_eq!(r1.len(), 2);
    assert_eq!(r1[0].layer_id, "layer-1");
    assert_eq!(r1[0].quantity, 50.0);
    assert_eq!(r1[1].layer_id, "layer-2");
    assert_eq!(r1[1].quantity, 10.0);
}

#[test]
fn simulate_fifo_consumption_empty_layers_determinism() {
    let r1 = simulate_fifo_consumption("p", &[], 0.0).unwrap();
    let r2 = simulate_fifo_consumption("p", &[], 0.0).unwrap();
    assert!(json_eq(&r1, &r2), "empty consumption must be deterministic");
    assert!(r1.is_empty());
}

#[test]
fn simulate_fifo_consumption_exact_layer_boundary_determinism() {
    let layers = vec![("l1".into(), 300.0, 20.0), ("l2".into(), 400.0, 30.0)];
    let r1 = simulate_fifo_consumption("p", &layers, 20.0).unwrap();
    let r2 = simulate_fifo_consumption("p", &layers, 20.0).unwrap();
    assert!(
        json_eq(&r1, &r2),
        "exact layer boundary must be deterministic"
    );
    assert_eq!(r1.len(), 1);
    assert_eq!(r1[0].layer_id, "l1");
    assert_eq!(r1[0].quantity, 20.0);
}

#[test]
fn simulate_fifo_consumption_insufficient_stock_determinism() {
    let layers = vec![("l1".into(), 300.0, 10.0)];
    let r1 = simulate_fifo_consumption("p", &layers, 20.0);
    let r2 = simulate_fifo_consumption("p", &layers, 20.0);
    assert_eq!(
        r1.is_err(),
        r2.is_err(),
        "insufficient stock error must be deterministic"
    );
    assert!(r1.is_err());
}

#[test]
fn compute_meal_fifo_costs_determinism() {
    let meals = vec![MealSectionInput {
        meal_type: MealType::Breakfast,
        staff_24h_count: 0,
        staff_8h_count: 0,
        reservation_count: 0,
        mission_count: 0,
        guest_count: 0,
        items: vec![
            ConsumptionItemInput {
                product_id: "prod-a".into(),
                quantity: 10.0,
            },
            ConsumptionItemInput {
                product_id: "prod-b".into(),
                quantity: 5.0,
            },
        ],
    }];

    let mut inventory: HashMap<String, Vec<(String, f64, f64)>> = HashMap::new();
    inventory.insert(
        "prod-a".into(),
        vec![("l1".into(), 400.0, 100.0), ("l2".into(), 500.0, 50.0)],
    );
    inventory.insert("prod-b".into(), vec![("l3".into(), 300.0, 30.0)]);

    let r1 = compute_meal_fifo_costs(&meals, |pid, qty| {
        consume_from_map(&mut inventory, pid, qty)
    })
    .unwrap();
    let r2 = compute_meal_fifo_costs(&meals, |pid, qty| {
        consume_from_map(&mut inventory, pid, qty)
    })
    .unwrap();

    assert_eq!(
        r1.total_cost, r2.total_cost,
        "total cost must be deterministic"
    );
    assert_eq!(r1.meals.len(), r2.meals.len(), "meal count must match");
    assert_eq!(r1.meals[0].products.len(), r2.meals[0].products.len());
    assert!(
        json_eq(
            &r1.meals[0].products[0].portions,
            &r2.meals[0].products[0].portions
        ),
        "portions must be deterministic"
    );
    assert!(
        format!("{:?}", r1) == format!("{:?}", r2),
        "full computation must be deterministic"
    );
}

#[test]
fn compute_meal_fifo_costs_empty_meals_determinism() {
    let r1 = compute_meal_fifo_costs(&[], |_: &str, _: f64| Ok(vec![])).unwrap();
    let r2 = compute_meal_fifo_costs(&[], |_: &str, _: f64| Ok(vec![])).unwrap();

    assert_eq!(format!("{:?}", r1), format!("{:?}", r2));
    assert_eq!(r1.total_cost, 0.0);
    assert!(r1.meals.is_empty());
}

#[test]
fn compute_entry_hash_determinism() {
    let entry = NewAuditEntry {
        id: "entry-1".into(),
        user_id: "user-1".into(),
        username: "test-user".into(),
        action: "LOGIN".into(),
        entity_type: "Session".into(),
        entity_id: Some("sess-1".into()),
        entity_name: None,
        old_value: None,
        new_value: None,
        session_id: Some("sess-1".into()),
        timestamp: "2026-05-29T10:00:00Z".into(),
        status: "success".into(),
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
    };

    let h1 = compute_entry_hash(None, &entry);
    let h2 = compute_entry_hash(None, &entry);

    assert_eq!(h1, h2, "compute_entry_hash must be deterministic");
    assert_eq!(h1.len(), 64, "SHA-256 hex output must be 64 chars");

    let entry2 = NewAuditEntry {
        id: "entry-2".into(),
        previous_hash: Some(h1.clone()),
        ..entry
    };

    let h3 = compute_entry_hash(Some(&h1), &entry2);
    let h4 = compute_entry_hash(Some(&h1), &entry2);

    assert_eq!(h3, h4, "chained hash must be deterministic");
}

#[test]
fn compute_entry_hash_with_previous_determinism() {
    let entry = NewAuditEntry {
        id: "entry-1".into(),
        user_id: "user-1".into(),
        username: "test-user".into(),
        action: "FISCAL_CLOSE".into(),
        entity_type: "FiscalYear".into(),
        entity_id: Some("2025".into()),
        entity_name: Some("Fiscal Year 2025".into()),
        old_value: Some("open".into()),
        new_value: Some("closed".into()),
        session_id: Some("sess-2".into()),
        timestamp: "2026-05-29T11:00:00Z".into(),
        status: "success".into(),
        error_message: None,
        metadata: Some(r#"{"reason": "year-end"}"#.into()),
        previous_hash: Some("abc123".into()),
        entry_hash: None,
        event_type: Some("FiscalYearClosed".into()),
        actor_id: Some("user-1".into()),
        target_type: Some("FiscalYear".into()),
        target_id: Some("2025".into()),
        fiscal_year: Some(2025),
        before_snapshot: Some(r#"{"status": "open"}"#.into()),
        after_snapshot: Some(r#"{"status": "closed"}"#.into()),
        node_id: Some("node-1".into()),
        details: Some("Year-end closing".into()),
    };

    let h1 = compute_entry_hash(Some("abc123"), &entry);
    let h2 = compute_entry_hash(Some("abc123"), &entry);

    assert_eq!(h1, h2, "full-entry hash must be deterministic");
    assert_eq!(h1.len(), 64);
}

#[test]
fn check_fifo_consumption_order_determinism() {
    let consumptions = vec![
        LayerAtConsumption {
            layer_id: "l1".into(),
            received_at: "2026-01-01".into(),
            quantity_consumed: 10.0,
            quantity_remaining_before: 50.0,
            fifo_order_index: 0,
        },
        LayerAtConsumption {
            layer_id: "l2".into(),
            received_at: "2026-01-02".into(),
            quantity_consumed: 5.0,
            quantity_remaining_before: 30.0,
            fifo_order_index: 1,
        },
    ];
    let pre = vec![
        PreConsumptionLayer {
            layer_id: "l1".into(),
            received_at: "2026-01-01".into(),
            quantity_remaining: 50.0,
        },
        PreConsumptionLayer {
            layer_id: "l2".into(),
            received_at: "2026-01-02".into(),
            quantity_remaining: 30.0,
        },
    ];

    let v1 = check_fifo_consumption_order(consumptions.clone(), pre.clone());
    let v2 = check_fifo_consumption_order(consumptions, pre);

    assert_eq!(v1, v2, "fifo order check must be deterministic");
    assert!(
        v1.is_empty(),
        "in-order consumption must have no violations"
    );
}

#[test]
fn check_fifo_consumption_order_skipped_layer_determinism() {
    let consumptions = vec![LayerAtConsumption {
        layer_id: "l2".into(),
        received_at: "2026-01-02".into(),
        quantity_consumed: 5.0,
        quantity_remaining_before: 30.0,
        fifo_order_index: 1,
    }];
    let pre = vec![
        PreConsumptionLayer {
            layer_id: "l1".into(),
            received_at: "2026-01-01".into(),
            quantity_remaining: 50.0,
        },
        PreConsumptionLayer {
            layer_id: "l2".into(),
            received_at: "2026-01-02".into(),
            quantity_remaining: 30.0,
        },
    ];

    let v1 = check_fifo_consumption_order(consumptions.clone(), pre.clone());
    let v2 = check_fifo_consumption_order(consumptions, pre);

    assert_eq!(v1, v2, "violation detection must be deterministic");
    assert!(!v1.is_empty(), "skipped layer must produce violations");
    assert_eq!(v1.len(), 1);
}

#[test]
fn check_stock_non_negative_determinism() {
    let layers = vec![
        LayerQuantity {
            layer_id: "l1".into(),
            quantity: 50.0,
        },
        LayerQuantity {
            layer_id: "l2".into(),
            quantity: -5.0,
        },
    ];
    let balances = vec![
        ProductBalance {
            product_label: "prod-a".into(),
            balance: 100.0,
        },
        ProductBalance {
            product_label: "prod-b".into(),
            balance: -10.0,
        },
    ];

    let v1 = check_stock_non_negative(layers.clone(), balances.clone());
    let v2 = check_stock_non_negative(layers, balances);

    assert_eq!(v1, v2, "stock non-negative check must be deterministic");
    assert_eq!(v1.len(), 2, "must detect 2 violations");
}

#[test]
fn check_layer_quantity_determinism() {
    let v1 = check_layer_quantity("l1", -5.0);
    let v2 = check_layer_quantity("l1", -5.0);
    assert_eq!(v1, v2, "layer quantity check must be deterministic");
    assert!(!v1.is_empty());

    let v3 = check_layer_quantity("l2", 10.0);
    let v4 = check_layer_quantity("l2", 10.0);
    assert_eq!(v3, v4);
    assert!(v3.is_empty());
}

#[test]
fn check_product_balance_determinism() {
    let v1 = check_product_balance("prod-a", -1.0);
    let v2 = check_product_balance("prod-a", -1.0);
    assert_eq!(v1, v2, "product balance check must be deterministic");
    assert!(!v1.is_empty());

    let v3 = check_product_balance("prod-b", 0.0);
    let v4 = check_product_balance("prod-b", 0.0);
    assert_eq!(v3, v4);
    assert!(v3.is_empty());
}

#[test]
fn check_origin_immutable_determinism() {
    let layers = vec![
        OriginYearRecord {
            layer_id: "l1".into(),
            expected_origin_year: 2025,
            actual_origin_year: 2025,
        },
        OriginYearRecord {
            layer_id: "l2".into(),
            expected_origin_year: 2024,
            actual_origin_year: 2025,
        },
    ];

    let v1 = check_origin_immutable(layers.clone());
    let v2 = check_origin_immutable(layers);

    assert_eq!(v1, v2, "origin immutable check must be deterministic");
    assert_eq!(v1.len(), 1, "must detect 1 origin change");
}

#[test]
fn check_single_open_fiscal_year_determinism() {
    let years = vec![
        FiscalYearEntry {
            year: 2023,
            status: "archived".into(),
        },
        FiscalYearEntry {
            year: 2024,
            status: "closed".into(),
        },
        FiscalYearEntry {
            year: 2025,
            status: "open".into(),
        },
        FiscalYearEntry {
            year: 2026,
            status: "open".into(),
        },
    ];

    let v1 = check_single_open_fiscal_year(years.clone());
    let v2 = check_single_open_fiscal_year(years);

    assert_eq!(
        v1, v2,
        "single open fiscal year check must be deterministic"
    );
    assert_eq!(v1.len(), 1, "must detect 2 open years");
}

#[test]
fn check_single_open_fiscal_year_no_open_year_determinism() {
    let years = vec![FiscalYearEntry {
        year: 2025,
        status: "closed".into(),
    }];

    let v1 = check_single_open_fiscal_year(years.clone());
    let v2 = check_single_open_fiscal_year(years);

    assert_eq!(v1, v2, "no-open-year check must be deterministic");
    assert_eq!(v1.len(), 1);
}

#[test]
fn check_single_open_fiscal_year_valid_determinism() {
    let years = vec![FiscalYearEntry {
        year: 2025,
        status: "open".into(),
    }];

    let v1 = check_single_open_fiscal_year(years.clone());
    let v2 = check_single_open_fiscal_year(years);

    assert_eq!(v1, v2);
    assert!(v1.is_empty(), "exactly one open year is valid");
}

#[test]
fn check_audit_chain_integrity_determinism() {
    let entries = vec![
        ChainEntry {
            id: "e1".into(),
            previous_hash: None,
            entry_hash: Some("hash1".into()),
        },
        ChainEntry {
            id: "e2".into(),
            previous_hash: Some("hash1".into()),
            entry_hash: Some("hash2".into()),
        },
        ChainEntry {
            id: "e3".into(),
            previous_hash: Some("hash2".into()),
            entry_hash: Some("hash3".into()),
        },
    ];

    let v1 = check_audit_chain_integrity(entries.clone());
    let v2 = check_audit_chain_integrity(entries);

    assert_eq!(v1, v2, "audit chain integrity check must be deterministic");
    assert!(v1.is_empty(), "valid chain must have no violations");
}

#[test]
fn check_audit_chain_integrity_break_determinism() {
    let entries = vec![
        ChainEntry {
            id: "e1".into(),
            previous_hash: None,
            entry_hash: Some("hash1".into()),
        },
        ChainEntry {
            id: "e2".into(),
            previous_hash: Some("WRONG_HASH".into()),
            entry_hash: Some("hash2".into()),
        },
    ];

    let v1 = check_audit_chain_integrity(entries.clone());
    let v2 = check_audit_chain_integrity(entries);

    assert_eq!(v1, v2, "break detection must be deterministic");
    assert_eq!(v1.len(), 1, "must detect 1 linkage break");
}

#[test]
fn check_audit_chain_integrity_missing_hash_determinism() {
    let entries = vec![ChainEntry {
        id: "e1".into(),
        previous_hash: Some("prev".into()),
        entry_hash: None,
    }];

    let v1 = check_audit_chain_integrity(entries.clone());
    let v2 = check_audit_chain_integrity(entries);

    assert_eq!(v1, v2);
    assert_eq!(v1.len(), 1, "must detect 1 missing entry hash");
}

#[test]
fn validate_replay_ordering_determinism() {
    let tx_id = Uuid::new_v4();
    let events = vec![
        StoredEvent {
            sequence_number: 1,
            transaction_id: tx_id,
            category: grpc_lib::domain::events::EventCategory::Stock,
            event: DomainEvent::StockMovementRecorded {
                movement_id: "m1".into(),
                account: "acc-1".into(),
                actor_user_id: "u1".into(),
            },
        },
        StoredEvent {
            sequence_number: 2,
            transaction_id: tx_id,
            category: grpc_lib::domain::events::EventCategory::Stock,
            event: DomainEvent::FifoLayerConsumed {
                layer_id: "l1".into(),
                quantity: 10.0,
                unit_cost: 400.0,
            },
        },
    ];

    let r1 = validate_replay_ordering(&events);
    let r2 = validate_replay_ordering(&events);

    assert_eq!(r1, r2, "replay ordering validation must be deterministic");
    assert!(r1, "correct order must validate");
}

#[test]
fn validate_replay_ordering_reversed_transaction_determinism() {
    let tx_high = Uuid::from_u128(2);
    let tx_low = Uuid::from_u128(1);
    let events = vec![
        StoredEvent {
            sequence_number: 1,
            transaction_id: tx_high,
            category: grpc_lib::domain::events::EventCategory::Fiscal,
            event: DomainEvent::FiscalYearClosed { year: 2025 },
        },
        StoredEvent {
            sequence_number: 1,
            transaction_id: tx_low,
            category: grpc_lib::domain::events::EventCategory::Fiscal,
            event: DomainEvent::FiscalYearClosed { year: 2024 },
        },
    ];

    let r1 = validate_replay_ordering(&events);
    let r2 = validate_replay_ordering(&events);

    assert_eq!(r1, r2, "reversed detection must be deterministic");
    assert!(!r1, "reversed transactions (2 before 1) must fail");
}

#[test]
fn validate_transaction_sequences_determinism() {
    let tx_id = Uuid::new_v4();
    let events = vec![
        StoredEvent {
            sequence_number: 1,
            transaction_id: tx_id,
            category: grpc_lib::domain::events::EventCategory::Stock,
            event: DomainEvent::StockMovementRecorded {
                movement_id: "m1".into(),
                account: "acc-1".into(),
                actor_user_id: "u1".into(),
            },
        },
        StoredEvent {
            sequence_number: 2,
            transaction_id: tx_id,
            category: grpc_lib::domain::events::EventCategory::Stock,
            event: DomainEvent::FifoLayerConsumed {
                layer_id: "l1".into(),
                quantity: 10.0,
                unit_cost: 400.0,
            },
        },
        StoredEvent {
            sequence_number: 3,
            transaction_id: tx_id,
            category: grpc_lib::domain::events::EventCategory::Stock,
            event: DomainEvent::InventoryCorrected {
                product_id: "p1".into(),
                before_quantity: 100.0,
                after_quantity: 90.0,
                reason: "adjustment".into(),
            },
        },
    ];

    let r1 = validate_transaction_sequences(&events);
    let r2 = validate_transaction_sequences(&events);

    assert_eq!(r1, r2, "sequence validation must be deterministic");
    assert!(r1, "contiguous sequence must validate");
}

#[test]
fn validate_transaction_sequences_gap_determinism() {
    let tx_id = Uuid::new_v4();
    let events = vec![
        StoredEvent {
            sequence_number: 1,
            transaction_id: tx_id,
            category: grpc_lib::domain::events::EventCategory::Stock,
            event: DomainEvent::StockMovementRecorded {
                movement_id: "m1".into(),
                account: "acc-1".into(),
                actor_user_id: "u1".into(),
            },
        },
        StoredEvent {
            sequence_number: 3,
            transaction_id: tx_id,
            category: grpc_lib::domain::events::EventCategory::Stock,
            event: DomainEvent::FifoLayerConsumed {
                layer_id: "l1".into(),
                quantity: 10.0,
                unit_cost: 400.0,
            },
        },
    ];

    let r1 = validate_transaction_sequences(&events);
    let r2 = validate_transaction_sequences(&events);

    assert_eq!(r1, r2, "gap detection must be deterministic");
    assert!(!r1, "non-contiguous sequence must fail");
}

#[test]
fn event_buffer_properties_determinism() {
    let tx_id = Uuid::new_v4();
    let buf1 = EventBuffer::new(tx_id);
    let buf2 = EventBuffer::new(tx_id);

    assert_eq!(buf1.len(), buf2.len());
    assert_eq!(buf1.is_empty(), buf2.is_empty());
    assert!(buf1.is_empty());
    assert!(buf1.has_contiguous_sequence());
    assert!(buf2.has_contiguous_sequence());
    assert_eq!(buf1.transaction_id(), buf2.transaction_id());
}

#[test]
fn canonical_json_bytes_deterministic_key_ordering() {
    let value = json!({
        "z": "last",
        "a": "first",
        "m": "middle",
        "nested": {
            "b": 2,
            "a": 1
        }
    });

    let b1 = canonical_json_bytes(&value).unwrap();
    let b2 = canonical_json_bytes(&value).unwrap();

    assert_eq!(b1, b2, "canonical JSON must be byte-identical");

    let expected = br#"{"a":"first","m":"middle","nested":{"a":1,"b":2},"z":"last"}"#;
    assert_eq!(b1, expected, "keys must be sorted lexicographically");
}

#[test]
fn canonical_json_bytes_array_preserves_order() {
    let value = json!(["z", "a", "m"]);

    let b1 = canonical_json_bytes(&value).unwrap();
    let b2 = canonical_json_bytes(&value).unwrap();

    assert_eq!(b1, b2);

    let expected = br#"["z","a","m"]"#;
    assert_eq!(b1, expected, "arrays must preserve element order");
}

#[test]
fn canonical_json_bytes_nested_determinism() {
    let value = json!({
        "outer": {
            "inner": {
                "c": 3,
                "b": 2,
                "a": 1
            }
        },
        "list": [
            {"id": "z", "val": 1},
            {"id": "a", "val": 2}
        ]
    });

    let b1 = canonical_json_bytes(&value).unwrap();
    let b2 = canonical_json_bytes(&value).unwrap();

    assert_eq!(b1, b2, "nested canonical JSON must be byte-identical");
    assert!(!b1.is_empty());
}

#[test]
fn canonical_json_bytes_numbers_and_bools_determinism() {
    let value = json!({
        "int": 42,
        "float": 123.456,
        "bool_true": true,
        "bool_false": false,
        "null_val": null
    });

    let b1 = canonical_json_bytes(&value).unwrap();
    let b2 = canonical_json_bytes(&value).unwrap();

    assert_eq!(b1, b2, "scalar JSON must be byte-identical");

    let expected =
        br#"{"bool_false":false,"bool_true":true,"float":123.456,"int":42,"null_val":null}"#;
    assert_eq!(b1, expected);
}

#[test]
fn canonical_json_bytes_empty_objects_arrays_determinism() {
    let value = json!({
        "empty_obj": {},
        "empty_arr": []
    });

    let b1 = canonical_json_bytes(&value).unwrap();
    let b2 = canonical_json_bytes(&value).unwrap();

    assert_eq!(b1, b2);

    let expected = br#"{"empty_arr":[],"empty_obj":{}}"#;
    assert_eq!(b1, expected);
}
