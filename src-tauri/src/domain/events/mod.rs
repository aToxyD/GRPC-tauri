use serde::{Deserialize, Serialize};
use std::str::FromStr;
use uuid::Uuid;

/// Categorization of domain events by domain area.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventCategory {
    Stock,
    Fiscal,
    Sync,
    Audit,
}

impl EventCategory {
    /// Serialize category as a static string for DB storage.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Stock => "Stock",
            Self::Fiscal => "Fiscal",
            Self::Sync => "Sync",
            Self::Audit => "Audit",
        }
    }
}

impl FromStr for EventCategory {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Stock" => Ok(Self::Stock),
            "Fiscal" => Ok(Self::Fiscal),
            "Sync" => Ok(Self::Sync),
            "Audit" => Ok(Self::Audit),
            other => Err(format!("invalid event category: {}", other)),
        }
    }
}

/// A domain event enriched with its transaction-scoped metadata.
///
/// The `(transaction_id, sequence_number)` pair is assigned at emission
/// time and provides deterministic ordering for replay and reconstruction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredEvent {
    pub sequence_number: u64,
    pub transaction_id: Uuid,
    pub category: EventCategory,
    pub event: DomainEvent,
}

impl StoredEvent {
    /// Serialize the event body as JSON for persistence.
    pub fn event_body_json(&self) -> serde_json::Result<String> {
        serde_json::to_string(&self.event)
    }

    /// The event type name string.
    pub fn event_type_str(&self) -> &'static str {
        self.event.event_name()
    }
}

/// All domain events in the system.
///
/// Events are:
/// - Emitted only by application services
/// - Buffered in-memory during transaction scope
/// - Assigned `(transaction_id, sequence_number)` at emission time
///
/// Repositories MUST NOT emit events.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum DomainEvent {
    // ── Stock Events ──
    StockMovementRecorded {
        movement_id: String,
        account: String,
        actor_user_id: String,
    },
    FifoLayerConsumed {
        layer_id: String,
        quantity: f64,
        unit_cost: f64,
    },
    InventoryCorrected {
        product_id: String,
        before_quantity: f64,
        after_quantity: f64,
        reason: String,
    },

    // ── Fiscal Events ──
    FiscalYearClosed {
        year: i32,
    },
    FiscalYearArchived {
        year: i32,
    },
    FiscalTransitionApplied {
        from_year: i32,
        to_year: i32,
    },

    // ── Sync Events ──
    SyncPackageImported {
        package_id: String,
        kind: String,
        source_node_id: Option<String>,
        sequence_number: Option<u64>,
    },
    SyncConflictDetected {
        conflict_id: i64,
        conflict_type: Option<String>,
        package_id: Option<String>,
        details: Option<String>,
    },
    SyncConflictResolved {
        conflict_id: i64,
        resolved_by: String,
        resolution: Option<String>,
    },

    // ── Audit Events ──
    AuditEventWritten {
        audit_event_id: i64,
    },
    AuditIntegrityBreach {
        details: String,
    },
}

impl DomainEvent {
    /// Categorize this event by domain area.
    pub fn category(&self) -> EventCategory {
        match self {
            Self::StockMovementRecorded { .. }
            | Self::FifoLayerConsumed { .. }
            | Self::InventoryCorrected { .. } => EventCategory::Stock,

            Self::FiscalYearClosed { .. }
            | Self::FiscalYearArchived { .. }
            | Self::FiscalTransitionApplied { .. } => EventCategory::Fiscal,

            Self::SyncPackageImported { .. }
            | Self::SyncConflictDetected { .. }
            | Self::SyncConflictResolved { .. } => EventCategory::Sync,

            Self::AuditEventWritten { .. } | Self::AuditIntegrityBreach { .. } => {
                EventCategory::Audit
            }
        }
    }

    /// Human-readable event name for logging and identification.
    pub fn event_name(&self) -> &'static str {
        match self {
            Self::StockMovementRecorded { .. } => "StockMovementRecorded",
            Self::FifoLayerConsumed { .. } => "FifoLayerConsumed",
            Self::InventoryCorrected { .. } => "InventoryCorrected",
            Self::FiscalYearClosed { .. } => "FiscalYearClosed",
            Self::FiscalYearArchived { .. } => "FiscalYearArchived",
            Self::FiscalTransitionApplied { .. } => "FiscalTransitionApplied",
            Self::SyncPackageImported { .. } => "SyncPackageImported",
            Self::SyncConflictDetected { .. } => "SyncConflictDetected",
            Self::SyncConflictResolved { .. } => "SyncConflictResolved",
            Self::AuditEventWritten { .. } => "AuditEventWritten",
            Self::AuditIntegrityBreach { .. } => "AuditIntegrityBreach",
        }
    }
}

/// Validate that a slice of events is in canonical replay order:
/// `(transaction_id ASC, sequence_number ASC)`.
///
/// Returns `true` if every adjacent pair respects the ordering invariant.
/// An empty or single-element slice trivially passes.
pub fn validate_replay_ordering(events: &[StoredEvent]) -> bool {
    for i in 1..events.len() {
        let prev = &events[i - 1];
        let cur = &events[i];
        if cur.transaction_id.as_u128() < prev.transaction_id.as_u128() {
            return false;
        }
        if cur.transaction_id == prev.transaction_id && cur.sequence_number <= prev.sequence_number
        {
            return false;
        }
    }
    true
}

/// Validate that all sequence numbers within each transaction are
/// contiguous (no gaps) and start at 1.
///
/// This is a stronger invariant than `validate_replay_ordering`: it
/// ensures no sequence numbers were skipped within any transaction.
pub fn validate_transaction_sequences(events: &[StoredEvent]) -> bool {
    let mut i = 0;
    while i < events.len() {
        let tx_id = events[i].transaction_id;
        let mut expected_seq = 1u64;
        let mut j = i;
        while j < events.len() && events[j].transaction_id == tx_id {
            if events[j].sequence_number != expected_seq {
                return false;
            }
            expected_seq += 1;
            j += 1;
        }
        i = j;
    }
    true
}

/// Transaction-scoped event buffer.
///
/// Collects domain events emitted during a single transactional scope.
/// On rollback the buffer is dropped (events discarded). On commit the
/// buffer is returned to the caller for persistence and dispatch.
#[derive(Debug, Clone)]
pub struct EventBuffer {
    transaction_id: Uuid,
    events: Vec<StoredEvent>,
    next_seq: u64,
}

impl EventBuffer {
    pub fn new(transaction_id: Uuid) -> Self {
        Self {
            transaction_id,
            events: Vec::new(),
            next_seq: 1,
        }
    }

    /// Allocate the next sequence number (contiguous, starts at 1 per transaction).
    fn allocate_sequence(&mut self) -> u64 {
        let seq = self.next_seq;
        self.next_seq += 1;
        seq
    }

    /// Push an event into the buffer and return its assigned sequence number.
    pub(crate) fn push(&mut self, event: DomainEvent) -> u64 {
        let seq = self.allocate_sequence();
        self.events.push(StoredEvent {
            sequence_number: seq,
            transaction_id: self.transaction_id,
            category: event.category(),
            event,
        });
        seq
    }

    /// Number of events in the buffer.
    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// Consume the buffer and return collected events in emission order.
    pub fn into_events(self) -> Vec<StoredEvent> {
        self.events
    }

    /// The transaction_id shared by all events in this buffer.
    pub fn transaction_id(&self) -> Uuid {
        self.transaction_id
    }

    /// Return events in emission order (borrowed).
    pub fn events(&self) -> &[StoredEvent] {
        &self.events
    }

    /// Verify that sequence numbers are contiguous from 1 to len.
    ///
    /// Returns `true` if sequence numbers form a perfect 1..N range.
    pub fn has_contiguous_sequence(&self) -> bool {
        if self.events.is_empty() {
            return true;
        }
        self.events
            .iter()
            .enumerate()
            .all(|(i, e)| e.sequence_number == (i as u64 + 1))
    }
}

/// Transaction-scoped event context.
///
/// Wraps a `DbExecutor` and an `EventBuffer`, providing services with
/// both database access and a domain-event emission API.
///
/// Created by `TransactionService::with_event_context` and valid only
/// within the closure scope.
pub struct EventContext<'a> {
    executor: crate::repositories::DbExecutor<'a>,
    buffer: EventBuffer,
}

impl<'a> EventContext<'a> {
    pub fn new(executor: crate::repositories::DbExecutor<'a>) -> Self {
        Self {
            executor,
            buffer: EventBuffer::new(Uuid::new_v4()),
        }
    }

    /// Access the database executor to perform repository operations.
    pub fn executor(&self) -> crate::repositories::DbExecutor<'a> {
        self.executor
    }

    /// Emit a domain event within the current transaction scope.
    ///
    /// Returns the assigned sequence number (contiguous, starts at 1).
    pub fn emit(&mut self, event: DomainEvent) -> u64 {
        self.buffer.push(event)
    }

    /// Consume the context and return the event buffer.
    pub fn into_buffer(self) -> EventBuffer {
        self.buffer
    }

    /// Number of events emitted so far.
    pub fn event_count(&self) -> usize {
        self.buffer.len()
    }

    /// Whether any events have been emitted.
    pub fn has_events(&self) -> bool {
        !self.buffer.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buffer_sequence_starts_at_one() {
        let mut buf = EventBuffer::new(Uuid::new_v4());
        let seq = buf.push(DomainEvent::FiscalYearClosed { year: 2024 });
        assert_eq!(seq, 1);
    }

    #[test]
    fn buffer_sequence_is_contiguous() {
        let mut buf = EventBuffer::new(Uuid::new_v4());
        assert_eq!(buf.push(DomainEvent::FiscalYearClosed { year: 2024 }), 1);
        assert_eq!(buf.push(DomainEvent::FiscalYearArchived { year: 2024 }), 2);
        assert_eq!(
            buf.push(DomainEvent::FiscalTransitionApplied {
                from_year: 2024,
                to_year: 2025
            }),
            3
        );
    }

    #[test]
    fn buffer_preserves_emission_order() {
        let mut buf = EventBuffer::new(Uuid::new_v4());
        buf.push(DomainEvent::FiscalYearClosed { year: 2024 });
        buf.push(DomainEvent::StockMovementRecorded {
            movement_id: "mov-1".into(),
            account: "Consumption".into(),
            actor_user_id: "user-1".into(),
        });
        buf.push(DomainEvent::SyncPackageImported {
            package_id: "pkg-1".into(),
            kind: "full".into(),
            source_node_id: None,
            sequence_number: None,
        });

        let events = buf.into_events();
        assert_eq!(events.len(), 3);
        assert!(matches!(
            events[0].event,
            DomainEvent::FiscalYearClosed { .. }
        ));
        assert!(matches!(
            events[1].event,
            DomainEvent::StockMovementRecorded { .. }
        ));
        assert!(matches!(
            events[2].event,
            DomainEvent::SyncPackageImported { .. }
        ));
    }

    #[test]
    fn buffer_contiguous_sequence_check() {
        let mut buf = EventBuffer::new(Uuid::new_v4());
        assert!(buf.has_contiguous_sequence());

        buf.push(DomainEvent::FiscalYearClosed { year: 2024 });
        buf.push(DomainEvent::FiscalYearArchived { year: 2024 });
        assert!(buf.has_contiguous_sequence());
    }

    #[test]
    fn buffer_shared_transaction_id() {
        let tx_id = Uuid::new_v4();
        let mut buf = EventBuffer::new(tx_id);
        buf.push(DomainEvent::FiscalYearClosed { year: 2024 });
        buf.push(DomainEvent::FiscalYearArchived { year: 2024 });

        assert_eq!(buf.transaction_id(), tx_id);
        for event in buf.events() {
            assert_eq!(event.transaction_id, tx_id);
        }
    }

    #[test]
    fn buffer_len_and_empty() {
        let mut buf = EventBuffer::new(Uuid::new_v4());
        assert!(buf.is_empty());
        assert_eq!(buf.len(), 0);

        buf.push(DomainEvent::FiscalYearClosed { year: 2024 });
        assert!(!buf.is_empty());
        assert_eq!(buf.len(), 1);
    }

    #[test]
    fn event_category_stock() {
        let e = DomainEvent::StockMovementRecorded {
            movement_id: "mov-1".into(),
            account: "Consumption".into(),
            actor_user_id: "u1".into(),
        };
        assert_eq!(e.category(), EventCategory::Stock);

        let e = DomainEvent::FifoLayerConsumed {
            layer_id: "layer-1".into(),
            quantity: 10.0,
            unit_cost: 5.0,
        };
        assert_eq!(e.category(), EventCategory::Stock);

        let e = DomainEvent::InventoryCorrected {
            product_id: "prod-1".into(),
            before_quantity: 10.0,
            after_quantity: 8.0,
            reason: "spoilage".into(),
        };
        assert_eq!(e.category(), EventCategory::Stock);
    }

    #[test]
    fn event_category_fiscal() {
        let e = DomainEvent::FiscalYearClosed { year: 2024 };
        assert_eq!(e.category(), EventCategory::Fiscal);

        let e = DomainEvent::FiscalYearArchived { year: 2024 };
        assert_eq!(e.category(), EventCategory::Fiscal);

        let e = DomainEvent::FiscalTransitionApplied {
            from_year: 2024,
            to_year: 2025,
        };
        assert_eq!(e.category(), EventCategory::Fiscal);
    }

    #[test]
    fn event_category_sync() {
        let e = DomainEvent::SyncPackageImported {
            package_id: "p1".into(),
            kind: "full".into(),
            source_node_id: None,
            sequence_number: None,
        };
        assert_eq!(e.category(), EventCategory::Sync);

        let e = DomainEvent::SyncConflictDetected {
            conflict_id: 1,
            conflict_type: None,
            package_id: None,
            details: None,
        };
        assert_eq!(e.category(), EventCategory::Sync);

        let e = DomainEvent::SyncConflictResolved {
            conflict_id: 1,
            resolved_by: "admin".into(),
            resolution: None,
        };
        assert_eq!(e.category(), EventCategory::Sync);
    }

    #[test]
    fn event_category_audit() {
        let e = DomainEvent::AuditEventWritten { audit_event_id: 1 };
        assert_eq!(e.category(), EventCategory::Audit);

        let e = DomainEvent::AuditIntegrityBreach {
            details: "hash mismatch".into(),
        };
        assert_eq!(e.category(), EventCategory::Audit);
    }

    #[test]
    fn event_name_is_accurate() {
        let e = DomainEvent::FiscalYearClosed { year: 2024 };
        assert_eq!(e.event_name(), "FiscalYearClosed");
    }

    #[test]
    fn event_context_emits_and_counts() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        let mut ctx = EventContext::new(crate::repositories::DbExecutor::Conn(&conn));
        assert!(!ctx.has_events());
        assert_eq!(ctx.event_count(), 0);

        ctx.emit(DomainEvent::FiscalYearClosed { year: 2024 });
        assert!(ctx.has_events());
        assert_eq!(ctx.event_count(), 1);

        ctx.emit(DomainEvent::FiscalYearArchived { year: 2024 });
        assert_eq!(ctx.event_count(), 2);
    }

    #[test]
    fn event_context_returns_buffer() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        let mut ctx = EventContext::new(crate::repositories::DbExecutor::Conn(&conn));
        ctx.emit(DomainEvent::FiscalYearClosed { year: 2024 });
        ctx.emit(DomainEvent::FiscalYearArchived { year: 2024 });

        let buf = ctx.into_buffer();
        assert_eq!(buf.len(), 2);
        assert!(buf.has_contiguous_sequence());
    }

    #[test]
    fn event_context_executor_is_copy() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        let executor = crate::repositories::DbExecutor::Conn(&conn);
        let _copy = executor;
        let _original = executor; // would fail if not Copy
    }

    #[test]
    fn domain_event_serde_round_trip() {
        let original = DomainEvent::FiscalYearClosed { year: 2024 };
        let json = serde_json::to_string(&original).unwrap();
        let restored: DomainEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(original.event_name(), restored.event_name());
        assert_eq!(original.category(), restored.category());
    }

    #[test]
    fn stored_event_serde_round_trip() {
        let tx_id = Uuid::new_v4();
        let stored = StoredEvent {
            sequence_number: 1,
            transaction_id: tx_id,
            category: EventCategory::Fiscal,
            event: DomainEvent::FiscalYearClosed { year: 2024 },
        };
        let json = serde_json::to_string(&stored).unwrap();
        let restored: StoredEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.sequence_number, 1);
        assert_eq!(restored.transaction_id, tx_id);
        assert_eq!(restored.category, EventCategory::Fiscal);
    }

    #[test]
    fn validate_replay_ordering_accepts_correct_order() {
        let tx = Uuid::new_v4();
        let events = vec![
            StoredEvent {
                sequence_number: 1,
                transaction_id: tx,
                category: EventCategory::Fiscal,
                event: DomainEvent::FiscalYearClosed { year: 2024 },
            },
            StoredEvent {
                sequence_number: 2,
                transaction_id: tx,
                category: EventCategory::Fiscal,
                event: DomainEvent::FiscalYearArchived { year: 2024 },
            },
        ];
        assert!(validate_replay_ordering(&events));
    }

    #[test]
    fn validate_replay_ordering_rejects_reversed_transaction() {
        // Use deterministic UUIDs where order is known: tx2 > tx1 by u128
        let tx1 = Uuid::from_u128(100);
        let tx2 = Uuid::from_u128(200);
        let events = vec![
            StoredEvent {
                sequence_number: 1,
                transaction_id: tx2,
                category: EventCategory::Fiscal,
                event: DomainEvent::FiscalYearClosed { year: 2024 },
            },
            StoredEvent {
                sequence_number: 1,
                transaction_id: tx1,
                category: EventCategory::Fiscal,
                event: DomainEvent::FiscalYearClosed { year: 2023 },
            },
        ];
        assert!(!validate_replay_ordering(&events));
    }

    #[test]
    fn validate_replay_ordering_rejects_same_tx_wrong_seq() {
        let tx = Uuid::from_u128(42);
        let events = vec![
            StoredEvent {
                sequence_number: 2,
                transaction_id: tx,
                category: EventCategory::Fiscal,
                event: DomainEvent::FiscalYearClosed { year: 2024 },
            },
            StoredEvent {
                sequence_number: 1,
                transaction_id: tx,
                category: EventCategory::Fiscal,
                event: DomainEvent::FiscalYearArchived { year: 2024 },
            },
        ];
        assert!(!validate_replay_ordering(&events));
    }

    #[test]
    fn validate_transaction_sequences_accepts_contiguous() {
        let tx = Uuid::new_v4();
        let events = vec![
            StoredEvent {
                sequence_number: 1,
                transaction_id: tx,
                category: EventCategory::Fiscal,
                event: DomainEvent::FiscalYearClosed { year: 2024 },
            },
            StoredEvent {
                sequence_number: 2,
                transaction_id: tx,
                category: EventCategory::Fiscal,
                event: DomainEvent::FiscalYearArchived { year: 2024 },
            },
        ];
        assert!(validate_transaction_sequences(&events));
    }

    #[test]
    fn validate_transaction_sequences_rejects_gaps() {
        let tx = Uuid::new_v4();
        let events = vec![
            StoredEvent {
                sequence_number: 1,
                transaction_id: tx,
                category: EventCategory::Fiscal,
                event: DomainEvent::FiscalYearClosed { year: 2024 },
            },
            StoredEvent {
                sequence_number: 3,
                transaction_id: tx,
                category: EventCategory::Fiscal,
                event: DomainEvent::FiscalYearArchived { year: 2024 },
            },
        ];
        assert!(!validate_transaction_sequences(&events));
    }

    #[test]
    fn validate_replay_ordering_trivially_passes_for_empty_slice() {
        assert!(validate_replay_ordering(&[]));
    }

    #[test]
    fn validate_transaction_sequences_trivially_passes_for_empty_slice() {
        assert!(validate_transaction_sequences(&[]));
    }
}
