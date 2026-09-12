use chrono::{Datelike, NaiveDate, Utc};
use grpc_lib::application::services::DailyReportService;
use grpc_lib::db::ConnectionFactory;
use grpc_lib::domain::events::{DomainEvent, EventBuffer};
use grpc_lib::models::{ConsumptionItemInput, DailyReportInput, MealSectionInput, MealType};
use grpc_lib::repositories::{executor::DbExecutor, DomainEventRepository, FifoLayerRepository};

fn seed_unit(ex: DbExecutor<'_>, id: &str, now: &str) {
    ex.execute(
        "INSERT INTO units (id, code, name, wilaya_code, created_at) VALUES (?1,'U01','Test Unit','01',?2)",
        rusqlite::params![id, now],
    ).expect("insert unit");
}

fn seed_product_and_stock(ex: DbExecutor<'_>, product_id: &str, name: &str, now: &str, year: i32) {
    ex.execute(
        "INSERT INTO products (id, name, base_price, year, created_at, purchase_unit, consumption_unit, conversion_factor, tva_classification) VALUES (?1,?2,0.0,?3,?4,1,1,1,0)",
        rusqlite::params![product_id, name, year, now],
    )
    .expect("insert product");
    ex.execute(
        "INSERT INTO inventory_stocks (id, product_id, quantity, unit, consumption_unit, last_updated, updated_at) VALUES (?1,?2,0.0,'unit',1,?3,?3)",
        rusqlite::params![format!("stock-{}", product_id), product_id, now],
    ).expect("insert inventory_stocks");
}

fn add_layer(
    fifo: &FifoLayerRepository,
    unit_id: &str,
    product_id: &str,
    qty: f64,
    unit_cost: f64,
    received_at: &str,
) -> String {
    fifo.create_layer(
        unit_id,
        product_id,
        "ORDER",
        None,
        unit_cost,
        qty,
        received_at,
        "system",
        2025,
    )
    .expect("create_layer failed")
}

fn meal(meal_type: MealType, items: Vec<(&str, f64)>) -> MealSectionInput {
    MealSectionInput {
        meal_type,
        staff_24h_count: 10,
        staff_8h_count: 5,
        reservation_count: 0,
        mission_count: 0,
        guest_count: 0,
        items: items
            .into_iter()
            .map(|(pid, qty)| ConsumptionItemInput {
                product_id: pid.to_string(),
                quantity: qty,
            })
            .collect(),
    }
}

fn sync_inventory_from_fifo(ex: DbExecutor<'_>, unit_id: &str, product_id: &str) {
    let fifo_qty: f64 = ex
        .query_row(
            "SELECT COALESCE(SUM(qty_remaining), 0.0)
         FROM fifo_stock_layers WHERE unit_id = ?1 AND product_id = ?2",
            rusqlite::params![unit_id, product_id],
            |row| row.get(0),
        )
        .unwrap_or(0.0);
    ex.execute(
        "UPDATE inventory_stocks SET quantity = ?1 WHERE product_id = ?2",
        rusqlite::params![fifo_qty, product_id],
    )
    .expect("sync inventory");
}

fn current_date_and_year() -> (NaiveDate, i32) {
    let date = Utc::now().date_naive();
    let year = date.year();
    (date, year)
}

fn setup_layers_and_create_report(
    layers: Vec<(f64, f64, &str)>,
    meals: Vec<(MealType, Vec<(&str, f64)>)>,
) -> (grpc_lib::db::Database, String, EventBuffer) {
    let mut db = ConnectionFactory::new_for_test().expect("test db init failed");
    let now = Utc::now().to_rfc3339();
    let (date, year) = current_date_and_year();
    let unit_id = uuid::Uuid::new_v4().to_string();
    let product_id = uuid::Uuid::new_v4().to_string();

    {
        let ex = db.executor();
        seed_unit(ex, &unit_id, &now);
        ex.execute(
            "INSERT OR IGNORE INTO fiscal_year_status (year, status, opened_at) VALUES (?1,'open',?2)",
            rusqlite::params![year, now],
        )
        .expect("seed fiscal year");
        seed_product_and_stock(ex, &product_id, "Rice", &now, year);
        let fifo = FifoLayerRepository::new(ex);
        for (qty, cost, received) in &layers {
            add_layer(&fifo, &unit_id, &product_id, *qty, *cost, received);
        }
    }
    sync_inventory_from_fifo(db.executor(), &unit_id, &product_id);

    let input = DailyReportInput {
        date,
        meals: meals
            .into_iter()
            .map(|(mt, items)| {
                let fixed: Vec<(&str, f64)> = items
                    .into_iter()
                    .map(|(_pid, qty)| (&*product_id, qty))
                    .collect();
                meal(mt, fixed)
            })
            .collect(),
    };

    let (_report_id, buffer) = db
        .with_event_persistence(|ctx| {
            let executor = ctx.executor();
            let rid = DailyReportService::new(executor).create_daily_report_impl(
                &input,
                Some(&unit_id),
                "system",
                "system",
                &mut |event| {
                    ctx.emit(event);
                },
            )?;
            Ok(rid)
        })
        .expect("create daily report");

    (db, unit_id, buffer)
}

// ── Test 1: FifoLayerConsumed events are emitted in consumption order ──
// Layer1: 10@400 (oldest), Layer2: 20@500, Layer3: 30@600 (newest)
// Breakfast: 10 (consumes Layer1 entirely)
// Lunch: 20 (consumes Layer2 entirely)
// Dinner: 10 (consumes 10 from Layer3)
// Expected events (6):
//   StockMovementRecorded (breakfast)
//   FifoLayerConsumed { L1, 10, 400 }
//   StockMovementRecorded (lunch)
//   FifoLayerConsumed { L2, 20, 500 }
//   StockMovementRecorded (dinner)
//   FifoLayerConsumed { L3, 10, 600 }

#[test]
fn fifo_layer_consumed_events_match_consumption_order() {
    let layers = vec![
        (10.0, 400.0, "2024-01-01T08:00:00Z"),
        (20.0, 500.0, "2024-01-02T08:00:00Z"),
        (30.0, 600.0, "2024-01-03T08:00:00Z"),
    ];
    let meals = vec![
        (MealType::Breakfast, vec![("A", 10.0)]),
        (MealType::Lunch, vec![("A", 20.0)]),
        (MealType::Dinner, vec![("A", 10.0)]),
    ];

    let (db, _unit_id, _buffer) = setup_layers_and_create_report(layers, meals);

    let repo = DomainEventRepository::new(db.executor());
    let all = repo.find_all_ordered().expect("find_all_ordered");

    // 3 meals x (1 StockMovement + 1 FifoLayerConsumed) = 6 events
    assert_eq!(all.len(), 6, "expected 6 events");

    // Event 0: StockMovementRecorded (breakfast)
    assert!(
        matches!(&all[0].event, DomainEvent::StockMovementRecorded { .. }),
        "event[0] should be StockMovementRecorded"
    );
    // Event 1: FifoLayerConsumed (layer1, 10@400) — breakfast consumed layer1 entirely
    assert!(
        matches!(
            &all[1].event,
            DomainEvent::FifoLayerConsumed {
                quantity: 10.0,
                unit_cost: 400.0,
                ..
            }
        ),
        "event[1] should be FifoLayerConsumed 10@400, got {:?}",
        all[1].event
    );
    // Event 2: StockMovementRecorded (lunch)
    assert!(
        matches!(&all[2].event, DomainEvent::StockMovementRecorded { .. }),
        "event[2] should be StockMovementRecorded"
    );
    // Event 3: FifoLayerConsumed (layer2, 20@500) — lunch consumed layer2 entirely
    assert!(
        matches!(
            &all[3].event,
            DomainEvent::FifoLayerConsumed {
                quantity: 20.0,
                unit_cost: 500.0,
                ..
            }
        ),
        "event[3] should be FifoLayerConsumed 20@500, got {:?}",
        all[3].event
    );
    // Event 4: StockMovementRecorded (dinner)
    assert!(
        matches!(&all[4].event, DomainEvent::StockMovementRecorded { .. }),
        "event[4] should be StockMovementRecorded"
    );
    // Event 5: FifoLayerConsumed (layer3, 10@600) — dinner consumed 10 from layer3
    assert!(
        matches!(
            &all[5].event,
            DomainEvent::FifoLayerConsumed {
                quantity: 10.0,
                unit_cost: 600.0,
                ..
            }
        ),
        "event[5] should be FifoLayerConsumed 10@600, got {:?}",
        all[5].event
    );
}

// ── Test 2: Multi-layer split within a single meal ──
// Layer1: 10@400, Layer2: 20@500
// Breakfast: 25 (spans both layers)
// Expected events (3):
//   StockMovementRecorded
//   FifoLayerConsumed { L1, 10, 400 }
//   FifoLayerConsumed { L2, 15, 500 }

#[test]
fn multi_layer_split_within_single_meal() {
    let layers = vec![
        (10.0, 400.0, "2024-01-01T08:00:00Z"),
        (20.0, 500.0, "2024-01-02T08:00:00Z"),
    ];
    let meals = vec![(MealType::Breakfast, vec![("A", 25.0)])];

    let (db, _unit_id, _buffer) = setup_layers_and_create_report(layers, meals);

    let repo = DomainEventRepository::new(db.executor());
    let all = repo.find_all_ordered().expect("find_all_ordered");

    // 1 meal x (1 StockMovement + 2 layer portions) = 3 events
    assert_eq!(all.len(), 3, "expected 3 events");

    assert!(
        matches!(&all[0].event, DomainEvent::StockMovementRecorded { .. }),
        "event[0] should be StockMovementRecorded"
    );
    assert!(
        matches!(
            &all[1].event,
            DomainEvent::FifoLayerConsumed {
                quantity: 10.0,
                unit_cost: 400.0,
                ..
            }
        ),
        "event[1] should be FifoLayerConsumed 10@400, got {:?}",
        all[1].event
    );
    assert!(
        matches!(
            &all[2].event,
            DomainEvent::FifoLayerConsumed {
                quantity: 15.0,
                unit_cost: 500.0,
                ..
            }
        ),
        "event[2] should be FifoLayerConsumed 15@500, got {:?}",
        all[2].event
    );
}

// ── Test 3: Replayed events match original emission order ──

#[test]
fn replayed_fifo_events_match_original_emission_order() {
    let layers = vec![
        (10.0, 400.0, "2024-01-01T08:00:00Z"),
        (20.0, 500.0, "2024-01-02T08:00:00Z"),
        (30.0, 600.0, "2024-01-03T08:00:00Z"),
    ];
    let meals = vec![
        (MealType::Breakfast, vec![("A", 10.0)]),
        (MealType::Lunch, vec![("A", 20.0)]),
        (MealType::Dinner, vec![("A", 10.0)]),
    ];

    let (db, _unit_id, buffer) = setup_layers_and_create_report(layers, meals);

    // Verify EventBuffer has contiguous sequences
    assert!(buffer.has_contiguous_sequence());
    assert_eq!(buffer.len(), 6);

    // Replay via find_all_ordered
    let repo = DomainEventRepository::new(db.executor());
    let all = repo.find_all_ordered().expect("find_all_ordered");

    assert_eq!(all.len(), buffer.len());

    // Verify sequence_numbers match between buffer and persisted
    for (i, persisted) in all.iter().enumerate() {
        assert_eq!(
            persisted.sequence_number,
            buffer.events()[i].sequence_number,
            "sequence_number mismatch at index {}",
            i
        );
        assert_eq!(
            persisted.transaction_id,
            buffer.transaction_id(),
            "transaction_id mismatch at index {}",
            i
        );
    }

    // Verify ordering invariants: (transaction_id ASC, sequence_number ASC)
    for i in 1..all.len() {
        assert!(
            all[i].transaction_id.as_u128() >= all[i - 1].transaction_id.as_u128(),
            "transaction_id ordering violation at index {}",
            i
        );
        if all[i].transaction_id == all[i - 1].transaction_id {
            assert_eq!(
                all[i].sequence_number,
                all[i - 1].sequence_number + 1,
                "sequence_number gap at index {}",
                i
            );
        }
    }
}

// ── Test 4: FIFO consumption events are rolled back on insufficient stock ──

#[test]
fn fifo_consumption_events_rollback_on_insufficient_stock() {
    let mut db = ConnectionFactory::new_for_test().expect("test db init failed");
    let now = Utc::now().to_rfc3339();
    let (date, year) = current_date_and_year();
    let unit_id = uuid::Uuid::new_v4().to_string();
    let product_id = uuid::Uuid::new_v4().to_string();

    // Only 10 units available but requesting 30 → should fail
    {
        let ex = db.executor();
        seed_unit(ex, &unit_id, &now);
        ex.execute(
            "INSERT OR IGNORE INTO fiscal_year_status (year, status, opened_at) VALUES (?1,'open',?2)",
            rusqlite::params![year, now],
        )
        .expect("seed fiscal year");
        seed_product_and_stock(ex, &product_id, "Rice", &now, year);
        let fifo = FifoLayerRepository::new(ex);
        add_layer(
            &fifo,
            &unit_id,
            &product_id,
            10.0,
            400.0,
            "2024-01-01T08:00:00Z",
        );
    }
    sync_inventory_from_fifo(db.executor(), &unit_id, &product_id);

    let input = DailyReportInput {
        date,
        meals: vec![meal(MealType::Breakfast, vec![(&product_id, 30.0)])],
    };

    let result: Result<(String, EventBuffer), grpc_lib::errors::AppError> = db
        .with_event_persistence(|ctx| {
            let executor = ctx.executor();
            let rid = DailyReportService::new(executor).create_daily_report_impl(
                &input,
                Some(&unit_id),
                "system",
                "system",
                &mut |event| {
                    ctx.emit(event);
                },
            )?;
            Ok(rid)
        });

    assert!(result.is_err(), "expected error due to insufficient stock");

    // Verify no events were persisted
    let count: i64 = db
        .get_connection()
        .query_row("SELECT COUNT(*) FROM domain_events", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 0, "no events should survive rollback");

    // Verify no stock movements were recorded
    let movement_count: i64 = db
        .get_connection()
        .query_row("SELECT COUNT(*) FROM stock_movements", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        movement_count, 0,
        "no stock movements should survive rollback"
    );

    // Verify FIFO layers are unchanged
    let fifo_qty: i64 = db
        .executor()
        .query_row(
            "SELECT COALESCE(SUM(qty_remaining), 0) FROM fifo_stock_layers WHERE unit_id = ?1 AND product_id = ?2",
            rusqlite::params![unit_id, product_id],
            |row| row.get(0),
        )
        .unwrap_or(-1);
    assert!(
        ((fifo_qty as f64) / 1000.0 - 10.0).abs() < 0.01,
        "FIFO remaining should be unchanged (10.0), got {}",
        fifo_qty as f64 / 1000.0
    );
}

// ── Test 5: Same layer consumed across multiple meals ──
// Single large layer: 100@500
// Breakfast: 30, Lunch: 40, Dinner: 20
// All portions come from the same layer
// Expected: 3 StockMovementRecorded + 3 FifoLayerConsumed (all same layer_id)

#[test]
fn same_layer_consumed_across_multiple_meals() {
    let layers = vec![(100.0, 500.0, "2024-01-01T08:00:00Z")];
    let meals = vec![
        (MealType::Breakfast, vec![("A", 30.0)]),
        (MealType::Lunch, vec![("A", 40.0)]),
        (MealType::Dinner, vec![("A", 20.0)]),
    ];

    let (db, _unit_id, buffer) = setup_layers_and_create_report(layers, meals);

    assert_eq!(buffer.len(), 6, "expected 6 events");

    let repo = DomainEventRepository::new(db.executor());
    let all = repo.find_all_ordered().expect("find_all_ordered");

    assert_eq!(all.len(), 6);

    // All FifoLayerConsumed events should reference the same layer
    // Events: S-SRM, S-FLC, S-SRM, S-FLC, S-SRM, S-FLC
    // (indices 1, 3, 5 are FifoLayerConsumed)
    let layer_ids: Vec<&str> = all
        .iter()
        .filter_map(|e| match &e.event {
            DomainEvent::FifoLayerConsumed { layer_id, .. } => Some(layer_id.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(layer_ids.len(), 3);
    assert_eq!(
        layer_ids[0], layer_ids[1],
        "all portions should share layer_id"
    );
    assert_eq!(
        layer_ids[0], layer_ids[2],
        "all portions should share layer_id"
    );

    // Verify total consumed quantity
    let total_consumed: f64 = all
        .iter()
        .filter_map(|e| match &e.event {
            DomainEvent::FifoLayerConsumed { quantity, .. } => Some(*quantity),
            _ => None,
        })
        .sum();
    assert!(
        (total_consumed - 90.0).abs() < 0.01,
        "total consumed should be 90, got {}",
        total_consumed
    );
}

// ── Test 6: Multiple products maintain independent FIFO ordering ──
// Two products, each with their own layers
// Product A layers: L1(10@400), L2(20@500)
// Product B layers: L1(15@300), L2(25@600)
// Breakfast: A=10, B=15
// Lunch: A=10, B=10
// Expected events (8):
//   StockMovementRecorded(A-breakfast)
//   FifoLayerConsumed { L1-A, 10, 400 }
//   StockMovementRecorded(B-breakfast)
//   FifoLayerConsumed { L1-B, 15, 300 }
//   StockMovementRecorded(A-lunch)
//   FifoLayerConsumed { L2-A, 10, 500 }  ← A's layer2, consumed during lunch
//   StockMovementRecorded(B-lunch)
//   FifoLayerConsumed { L2-B, 10, 600 }  ← B's layer2, consumed during lunch

#[test]
fn multiple_products_independent_fifo_ordering() {
    let mut db = ConnectionFactory::new_for_test().expect("test db init failed");
    let now = Utc::now().to_rfc3339();
    let (date, year) = current_date_and_year();
    let unit_id = uuid::Uuid::new_v4().to_string();
    let pa = uuid::Uuid::new_v4().to_string();
    let pb = uuid::Uuid::new_v4().to_string();

    {
        let ex = db.executor();
        seed_unit(ex, &unit_id, &now);
        ex.execute(
            "INSERT OR IGNORE INTO fiscal_year_status (year, status, opened_at) VALUES (?1,'open',?2)",
            rusqlite::params![year, now],
        )
        .expect("seed fiscal year");
        seed_product_and_stock(ex, &pa, "ProductA", &now, year);
        seed_product_and_stock(ex, &pb, "ProductB", &now, year);
        let fifo = FifoLayerRepository::new(ex);
        // Product A layers
        add_layer(&fifo, &unit_id, &pa, 10.0, 400.0, "2024-01-01T08:00:00Z");
        add_layer(&fifo, &unit_id, &pa, 20.0, 500.0, "2024-01-02T08:00:00Z");
        // Product B layers
        add_layer(&fifo, &unit_id, &pb, 15.0, 300.0, "2024-01-01T08:00:00Z");
        add_layer(&fifo, &unit_id, &pb, 25.0, 600.0, "2024-01-02T08:00:00Z");
    }
    sync_inventory_from_fifo(db.executor(), &unit_id, &pa);
    sync_inventory_from_fifo(db.executor(), &unit_id, &pb);

    let input = DailyReportInput {
        date,
        meals: vec![
            meal(MealType::Breakfast, vec![(&pa, 10.0), (&pb, 15.0)]),
            meal(MealType::Lunch, vec![(&pa, 10.0), (&pb, 10.0)]),
        ],
    };

    let (_report_id, _buffer) = db
        .with_event_persistence(|ctx| {
            let executor = ctx.executor();
            let rid = DailyReportService::new(executor).create_daily_report_impl(
                &input,
                Some(&unit_id),
                "system",
                "system",
                &mut |event| {
                    ctx.emit(event);
                },
            )?;
            Ok(rid)
        })
        .expect("create daily report");

    let repo = DomainEventRepository::new(db.executor());
    let all = repo.find_all_ordered().expect("find_all_ordered");

    // 2 meals x 2 products x (1 StockMovement + 1 FifoLayerConsumed) = 8 events
    assert_eq!(all.len(), 8, "expected 8 events");

    // Event pattern must be:
    // [0] SR(A-breakfast) [1] FLC(A@400) [2] SR(B-breakfast) [3] FLC(B@300)
    // [4] SR(A-lunch)     [5] FLC(A@500) [6] SR(B-lunch)     [7] FLC(B@600)

    assert!(
        matches!(&all[0].event, DomainEvent::StockMovementRecorded { .. }),
        "event[0] should be StockMovementRecorded"
    );
    assert!(
        matches!(
            &all[1].event,
            DomainEvent::FifoLayerConsumed {
                unit_cost: 400.0,
                ..
            }
        ),
        "event[1] should be FifoLayerConsumed @400 (A oldest layer)"
    );
    assert!(
        matches!(&all[2].event, DomainEvent::StockMovementRecorded { .. }),
        "event[2] should be StockMovementRecorded"
    );
    assert!(
        matches!(
            &all[3].event,
            DomainEvent::FifoLayerConsumed {
                unit_cost: 300.0,
                ..
            }
        ),
        "event[3] should be FifoLayerConsumed @300 (B oldest layer)"
    );
    assert!(
        matches!(&all[4].event, DomainEvent::StockMovementRecorded { .. }),
        "event[4] should be StockMovementRecorded"
    );
    assert!(
        matches!(
            &all[5].event,
            DomainEvent::FifoLayerConsumed {
                unit_cost: 500.0,
                ..
            }
        ),
        "event[5] should be FifoLayerConsumed @500 (A newer layer)"
    );
    assert!(
        matches!(&all[6].event, DomainEvent::StockMovementRecorded { .. }),
        "event[6] should be StockMovementRecorded"
    );
    assert!(
        matches!(
            &all[7].event,
            DomainEvent::FifoLayerConsumed {
                unit_cost: 600.0,
                ..
            }
        ),
        "event[7] should be FifoLayerConsumed @600 (B newer layer)"
    );
}

// ── Test 7: No events emitted when no layers consumed (empty meals) ──
// A single meal with no items (beneficiaries only) should succeed
// but emit no events

#[test]
fn empty_meal_emits_no_domain_events() {
    let mut db = ConnectionFactory::new_for_test().expect("test db init failed");
    let now = Utc::now().to_rfc3339();
    let (date, year) = current_date_and_year();
    let unit_id = uuid::Uuid::new_v4().to_string();

    {
        let ex = db.executor();
        seed_unit(ex, &unit_id, &now);
        ex.execute(
            "INSERT OR IGNORE INTO fiscal_year_status (year, status, opened_at) VALUES (?1,'open',?2)",
            rusqlite::params![year, now],
        )
        .expect("seed fiscal year");
    }

    let input = DailyReportInput {
        date,
        meals: vec![MealSectionInput {
            meal_type: MealType::Breakfast,
            staff_24h_count: 10,
            staff_8h_count: 5,
            reservation_count: 0,
            mission_count: 0,
            guest_count: 0,
            items: vec![],
        }],
    };

    let (_report_id, buffer) = db
        .with_event_persistence(|ctx| {
            let executor = ctx.executor();
            let rid = DailyReportService::new(executor).create_daily_report_impl(
                &input,
                Some(&unit_id),
                "system",
                "system",
                &mut |event| {
                    ctx.emit(event);
                },
            )?;
            Ok(rid)
        })
        .expect("create daily report with empty meal");

    assert!(
        buffer.is_empty(),
        "no events should be emitted for empty meal"
    );

    let count: i64 = db
        .get_connection()
        .query_row("SELECT COUNT(*) FROM domain_events", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 0, "no events should be persisted for empty meal");
}
