use grpc_lib::db::ConnectionFactory;
use grpc_lib::domain::events::{DomainEvent, EventBuffer};
use grpc_lib::errors::AppError;
use grpc_lib::repositories::DomainEventRepository;
use uuid::Uuid;

fn create_persisted_events(db: &mut grpc_lib::db::Database) -> (EventBuffer, Uuid) {
    let (_, buffer) = db
        .with_event_persistence(|ctx| {
            ctx.emit(DomainEvent::FiscalYearClosed { year: 2024 });
            ctx.emit(DomainEvent::FiscalTransitionApplied {
                from_year: 2024,
                to_year: 2025,
            });
            ctx.emit(DomainEvent::StockMovementRecorded {
                movement_id: "mov-10".into(),
                account: "Consumption".into(),
                actor_user_id: "admin".into(),
            });
            Ok(true)
        })
        .expect("persist should succeed");

    let tx_id = buffer.transaction_id();
    assert_eq!(buffer.len(), 3);
    assert!(buffer.has_contiguous_sequence());
    (buffer, tx_id)
}

#[test]
fn persisted_ordering_matches_emission() {
    let mut db = ConnectionFactory::new_for_test().unwrap();
    let (buffer, tx_id) = create_persisted_events(&mut db);

    let repo = DomainEventRepository::new(db.executor());
    let persisted = repo
        .find_by_transaction_id(tx_id)
        .expect("find_by_transaction_id");

    assert_eq!(persisted.len(), 3);
    assert!(
        matches!(
            persisted[0].event,
            DomainEvent::FiscalYearClosed { year: 2024 }
        ),
        "first persisted event should be FiscalYearClosed"
    );
    assert!(
        matches!(
            persisted[1].event,
            DomainEvent::FiscalTransitionApplied { .. }
        ),
        "second persisted event should be FiscalTransitionApplied"
    );
    assert!(
        matches!(
            persisted[2].event,
            DomainEvent::StockMovementRecorded { .. }
        ),
        "third persisted event should be StockMovementRecorded"
    );

    assert_eq!(
        persisted[0].sequence_number,
        buffer.events()[0].sequence_number
    );
    assert_eq!(
        persisted[1].sequence_number,
        buffer.events()[1].sequence_number
    );
    assert_eq!(
        persisted[2].sequence_number,
        buffer.events()[2].sequence_number
    );
}

#[test]
fn rollback_discards_all_persisted_events() {
    let mut db = ConnectionFactory::new_for_test().unwrap();

    let result: Result<(bool, EventBuffer), AppError> = db.with_event_persistence(|ctx| {
        ctx.emit(DomainEvent::FiscalYearClosed { year: 2024 });
        ctx.emit(DomainEvent::FiscalYearArchived { year: 2024 });
        Err(AppError::Internal("deliberate rollback".into()))
    });

    assert!(result.is_err(), "expected rollback");

    let count: i64 = db
        .get_connection()
        .query_row("SELECT COUNT(*) FROM domain_events", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 0, "no events should survive rollback");
}

#[test]
fn committed_transaction_persists_events_atomically() {
    let mut db = ConnectionFactory::new_for_test().unwrap();

    let (_, buffer) = db
        .with_event_persistence(|ctx| {
            ctx.emit(DomainEvent::FifoLayerConsumed {
                layer_id: "layer-1".into(),
                quantity: 50.0,
                unit_cost: 12.5,
            });
            ctx.emit(DomainEvent::InventoryCorrected {
                product_id: "prod-100".into(),
                before_quantity: 50.0,
                after_quantity: 45.0,
                reason: "spoilage".into(),
            });
            Ok(true)
        })
        .expect("commit should succeed");

    assert_eq!(buffer.len(), 2);

    let count: i64 = db
        .get_connection()
        .query_row("SELECT COUNT(*) FROM domain_events", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 2, "both events should be persisted");
}

#[test]
fn persisted_sequence_numbers_are_contiguous() {
    let mut db = ConnectionFactory::new_for_test().unwrap();
    let (_, tx_id) = create_persisted_events(&mut db);

    let repo = DomainEventRepository::new(db.executor());
    let persisted = repo
        .find_by_transaction_id(tx_id)
        .expect("find_by_transaction_id");

    for (i, event) in persisted.iter().enumerate() {
        assert_eq!(
            event.sequence_number,
            (i as u64 + 1),
            "sequence_number {} should be {}",
            event.sequence_number,
            i + 1
        );
    }
}

#[test]
fn replay_queries_ordered_by_transaction_id_sequence_number() {
    let mut db = ConnectionFactory::new_for_test().unwrap();

    let (_, buf1) = db
        .with_event_persistence(|ctx| {
            ctx.emit(DomainEvent::FiscalYearClosed { year: 2023 });
            Ok(true)
        })
        .expect("tx 1");

    let (_, buf2) = db
        .with_event_persistence(|ctx| {
            ctx.emit(DomainEvent::FiscalTransitionApplied {
                from_year: 2023,
                to_year: 2024,
            });
            ctx.emit(DomainEvent::FiscalYearClosed { year: 2024 });
            Ok(true)
        })
        .expect("tx 2");

    let tx1_id = buf1.transaction_id();
    let tx2_id = buf2.transaction_id();

    let repo = DomainEventRepository::new(db.executor());
    let all = repo.find_all_ordered().expect("find_all_ordered");

    assert_eq!(all.len(), 3);

    let tx1_events: Vec<_> = all
        .iter()
        .filter(|e| e.transaction_id == tx1_id)
        .collect();
    let tx2_events: Vec<_> = all
        .iter()
        .filter(|e| e.transaction_id == tx2_id)
        .collect();

    assert_eq!(tx1_events.len(), 1);
    assert_eq!(tx1_events[0].sequence_number, 1);

    assert_eq!(tx2_events.len(), 2);
    assert_eq!(tx2_events[0].sequence_number, 1);
    assert_eq!(tx2_events[1].sequence_number, 2);

    for (i, e) in all.iter().enumerate() {
        if i > 0 {
            assert!(
                e.transaction_id.as_u128() >= all[i - 1].transaction_id.as_u128(),
                "events must be ordered by transaction_id ASC"
            );
        }
    }

    let mut i = 0;
    while i < all.len() {
        let tx_id = all[i].transaction_id;
        let mut j = i;
        while j < all.len() && all[j].transaction_id == tx_id {
            let expected_seq = (j - i + 1) as u64;
            assert_eq!(all[j].sequence_number, expected_seq);
            j += 1;
        }
        i = j;
    }
}

#[test]
fn gap_detection_catches_corrupted_sequences() {
    let mut db = ConnectionFactory::new_for_test().unwrap();

    let (_, buffer) = db
        .with_event_persistence(|ctx| {
            ctx.emit(DomainEvent::FiscalYearClosed { year: 2024 });
            ctx.emit(DomainEvent::FiscalYearArchived { year: 2024 });
            Ok(true)
        })
        .expect("persist");

    let tx_id = buffer.transaction_id();

    let repo = DomainEventRepository::new(db.executor());
    let gaps_before = repo.detect_sequence_gaps().expect("detect_sequence_gaps");
    assert!(
        gaps_before.is_empty(),
        "no gaps before corruption: {:?}",
        gaps_before
    );

    let tx_str = tx_id.to_string();
    db.get_connection()
        .execute(
            "INSERT INTO domain_events (transaction_id, sequence_number, category, event_type, event_body)
             VALUES (?1, 10, 'Fiscal', 'FiscalYearClosed', '{\"type\":\"FiscalYearClosed\",\"data\":{\"year\":2024}}')",
            rusqlite::params![tx_str],
        )
        .expect("insert gap");

    let gaps_after = repo.detect_sequence_gaps().expect("detect_sequence_gaps");
    assert_eq!(gaps_after.len(), 1, "should detect exactly one gap");

    assert_eq!(gaps_after[0].transaction_id, tx_id);
    assert_eq!(gaps_after[0].actual_count, 3);
    assert_eq!(gaps_after[0].expected_count, 10);
}

#[test]
fn find_by_transaction_id_returns_empty_for_unknown_tx() {
    let db = ConnectionFactory::new_for_test().unwrap();
    let repo = DomainEventRepository::new(db.executor());
    let events = repo
        .find_by_transaction_id(Uuid::new_v4())
        .expect("find_by_transaction_id");
    assert!(events.is_empty());
}

#[test]
fn find_all_ordered_returns_empty_when_no_events() {
    let db = ConnectionFactory::new_for_test().unwrap();
    let repo = DomainEventRepository::new(db.executor());
    let all = repo.find_all_ordered().expect("find_all_ordered");
    assert!(all.is_empty());
}

#[test]
fn no_events_emitted_still_succeeds() {
    let mut db = ConnectionFactory::new_for_test().unwrap();

    let (result, buffer) = db
        .with_event_persistence(|_ctx| Ok(42))
        .expect("persist with no events");

    assert_eq!(result, 42);
    assert!(buffer.is_empty());

    let count: i64 = db
        .get_connection()
        .query_row("SELECT COUNT(*) FROM domain_events", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 0);
}

#[test]
fn multiple_events_in_single_tx_share_transaction_id() {
    let mut db = ConnectionFactory::new_for_test().unwrap();

    let (_, buffer) = db
        .with_event_persistence(|ctx| {
            ctx.emit(DomainEvent::FiscalYearClosed { year: 2024 });
            ctx.emit(DomainEvent::FifoLayerConsumed {
                layer_id: "layer-1".into(),
                quantity: 10.0,
                unit_cost: 5.0,
            });
            ctx.emit(DomainEvent::AuditEventWritten { audit_event_id: 99 });
            Ok(true)
        })
        .expect("persist");

    let tx_id = buffer.transaction_id();
    let repo = DomainEventRepository::new(db.executor());
    let persisted = repo
        .find_by_transaction_id(tx_id)
        .expect("find_by_transaction_id");

    assert_eq!(persisted.len(), 3);
    for event in &persisted {
        assert_eq!(event.transaction_id, tx_id);
    }
}

#[test]
fn event_category_is_preserved_through_persistence() {
    let mut db = ConnectionFactory::new_for_test().unwrap();

    let (_, buffer) = db
        .with_event_persistence(|ctx| {
            ctx.emit(DomainEvent::StockMovementRecorded {
                movement_id: "mov-1".into(),
                account: "IN".into(),
                actor_user_id: "admin".into(),
            });
            ctx.emit(DomainEvent::FiscalYearClosed { year: 2024 });
            ctx.emit(DomainEvent::SyncPackageImported {
                package_id: "pkg-1".into(),
                kind: "full".into(),
            });
            ctx.emit(DomainEvent::AuditEventWritten { audit_event_id: 1 });
            Ok(true)
        })
        .expect("persist");

    let tx_id = buffer.transaction_id();
    let repo = DomainEventRepository::new(db.executor());
    let persisted = repo
        .find_by_transaction_id(tx_id)
        .expect("find_by_transaction_id");

    assert_eq!(
        persisted[0].category,
        grpc_lib::domain::events::EventCategory::Stock
    );
    assert_eq!(
        persisted[1].category,
        grpc_lib::domain::events::EventCategory::Fiscal
    );
    assert_eq!(
        persisted[2].category,
        grpc_lib::domain::events::EventCategory::Sync
    );
    assert_eq!(
        persisted[3].category,
        grpc_lib::domain::events::EventCategory::Audit
    );
}
