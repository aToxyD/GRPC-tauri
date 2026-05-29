use crate::domain::events::DomainEvent;

pub enum InvalidationKind {
    Slugs(Vec<&'static str>),
    AllOpenFiscalYear,
}

pub fn invalidation_kind(event: &DomainEvent) -> InvalidationKind {
    match event {
        DomainEvent::StockMovementRecorded { .. } => InvalidationKind::Slugs(vec![
            "inventory-valuation",
            "stock-movement-ledger",
            "fifo-layer-report",
            "inventory-snapshot",
        ]),
        DomainEvent::InventoryCorrected { .. } => InvalidationKind::Slugs(vec![
            "inventory-valuation",
            "fifo-layer-report",
            "inventory-snapshot",
        ]),
        DomainEvent::FiscalYearClosed { .. } => InvalidationKind::Slugs(vec![
            "fiscal-year-summary",
            "fiscal-transition-report",
            "fiscal-closure-report",
        ]),
        DomainEvent::SyncPackageImported { .. } => InvalidationKind::AllOpenFiscalYear,
        _ => InvalidationKind::Slugs(vec![]),
    }
}

pub fn closed_year_from_event(event: &DomainEvent) -> Option<i32> {
    match event {
        DomainEvent::FiscalYearClosed { year } => Some(*year),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stock_movement_recorded_invalidates_inventory_reports() {
        let event = DomainEvent::StockMovementRecorded {
            movement_id: "m1".into(),
            account: "Consumption".into(),
            actor_user_id: "u1".into(),
        };
        match invalidation_kind(&event) {
            InvalidationKind::Slugs(slugs) => {
                assert!(slugs.contains(&"inventory-valuation"));
                assert!(slugs.contains(&"stock-movement-ledger"));
                assert!(slugs.contains(&"fifo-layer-report"));
                assert!(slugs.contains(&"inventory-snapshot"));
                assert_eq!(slugs.len(), 4);
            }
            _ => panic!("expected Slugs variant"),
        }
    }

    #[test]
    fn inventory_corrected_invalidates_inventory_reports() {
        let event = DomainEvent::InventoryCorrected {
            product_id: "p1".into(),
            before_quantity: 10.0,
            after_quantity: 8.0,
            reason: "spoilage".into(),
        };
        match invalidation_kind(&event) {
            InvalidationKind::Slugs(slugs) => {
                assert!(slugs.contains(&"inventory-valuation"));
                assert!(slugs.contains(&"fifo-layer-report"));
                assert!(slugs.contains(&"inventory-snapshot"));
                assert_eq!(slugs.len(), 3);
            }
            _ => panic!("expected Slugs variant"),
        }
    }

    #[test]
    fn fiscal_year_closed_invalidates_fiscal_reports() {
        let event = DomainEvent::FiscalYearClosed { year: 2024 };
        match invalidation_kind(&event) {
            InvalidationKind::Slugs(slugs) => {
                assert!(slugs.contains(&"fiscal-year-summary"));
                assert!(slugs.contains(&"fiscal-transition-report"));
                assert!(slugs.contains(&"fiscal-closure-report"));
                assert_eq!(slugs.len(), 3);
            }
            _ => panic!("expected Slugs variant"),
        }
    }

    #[test]
    fn sync_package_imported_uses_all_open() {
        let event = DomainEvent::SyncPackageImported {
            package_id: "pkg-1".into(),
            kind: "full".into(),
            source_node_id: None,
            sequence_number: None,
        };
        assert!(matches!(invalidation_kind(&event), InvalidationKind::AllOpenFiscalYear));

        let event = DomainEvent::SyncConflictDetected {
            conflict_id: 1,
            conflict_type: None,
            package_id: None,
            details: None,
        };
        assert!(matches!(invalidation_kind(&event), InvalidationKind::AllOpenFiscalYear));

        let event = DomainEvent::SyncConflictResolved {
            conflict_id: 1,
            resolved_by: "admin".into(),
            resolution: None,
        };
        assert!(matches!(invalidation_kind(&event), InvalidationKind::AllOpenFiscalYear));
    }

    #[test]
    fn sync_conflict_detected_does_not_invalidate() {
        let event = DomainEvent::SyncConflictDetected {
            conflict_id: 1,
            conflict_type: None,
            package_id: None,
            details: None,
        };
        match invalidation_kind(&event) {
            InvalidationKind::Slugs(slugs) => {
                assert!(slugs.is_empty());
            }
            _ => panic!("expected Slugs variant (empty)"),
        }
    }

    #[test]
    fn sync_conflict_resolved_does_not_invalidate() {
        let event = DomainEvent::SyncConflictResolved {
            conflict_id: 1,
            resolved_by: "admin".into(),
            resolution: None,
        };
        match invalidation_kind(&event) {
            InvalidationKind::Slugs(slugs) => assert!(slugs.is_empty()),
            _ => panic!("expected empty Slugs"),
        }
    }

    #[test]
    fn audit_event_written_does_not_invalidate() {
        let event = DomainEvent::AuditEventWritten { audit_event_id: 1 };
        match invalidation_kind(&event) {
            InvalidationKind::Slugs(slugs) => assert!(slugs.is_empty()),
            _ => panic!("expected empty Slugs"),
        }
    }

    #[test]
    fn audit_integrity_breach_does_not_invalidate() {
        let event = DomainEvent::AuditIntegrityBreach {
            details: "hash mismatch".into(),
        };
        match invalidation_kind(&event) {
            InvalidationKind::Slugs(slugs) => assert!(slugs.is_empty()),
            _ => panic!("expected empty Slugs"),
        }
    }

    #[test]
    fn fiscal_year_archived_does_not_invalidate() {
        let event = DomainEvent::FiscalYearArchived { year: 2024 };
        match invalidation_kind(&event) {
            InvalidationKind::Slugs(slugs) => assert!(slugs.is_empty()),
            _ => panic!("expected empty Slugs"),
        }
    }

    #[test]
    fn fiscal_transition_applied_does_not_invalidate() {
        let event = DomainEvent::FiscalTransitionApplied {
            from_year: 2024,
            to_year: 2025,
        };
        match invalidation_kind(&event) {
            InvalidationKind::Slugs(slugs) => assert!(slugs.is_empty()),
            _ => panic!("expected empty Slugs"),
        }
    }

    #[test]
    fn fifo_layer_consumed_does_not_invalidate() {
        let event = DomainEvent::FifoLayerConsumed {
            layer_id: "l1".into(),
            quantity: 5.0,
            unit_cost: 10.0,
        };
        match invalidation_kind(&event) {
            InvalidationKind::Slugs(slugs) => assert!(slugs.is_empty()),
            _ => panic!("expected empty Slugs"),
        }
    }

    #[test]
    fn closed_year_from_event_returns_correct_year() {
        let event = DomainEvent::FiscalYearClosed { year: 2024 };
        assert_eq!(closed_year_from_event(&event), Some(2024));
    }

    #[test]
    fn closed_year_from_event_none_for_other_events() {
        let event = DomainEvent::StockMovementRecorded {
            movement_id: "m1".into(),
            account: "IN".into(),
            actor_user_id: "u1".into(),
        };
        assert_eq!(closed_year_from_event(&event), None);
    }
}
