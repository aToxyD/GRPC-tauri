use super::sync_import_models::{ConflictResolutionOutcome, ResolutionPolicy};
use crate::application::sync_integrity::types::{ConflictMetadata, SyncConflict};

pub struct SyncConflictResolutionService;

impl SyncConflictResolutionService {
    pub fn resolve(
        conflict: &SyncConflict,
        _source_node_id: &str,
        _target_node_id: &str,
    ) -> ConflictResolutionOutcome {
        let policy = match conflict {
            SyncConflict::DuplicatePackage(meta) => {
                Self::resolve_duplicate_package(meta)
            }
            SyncConflict::StaleImport(meta) => {
                Self::resolve_stale_import(meta)
            }
            SyncConflict::ReplayAttempt(meta) => {
                Self::resolve_replay_attempt(meta)
            }
            SyncConflict::SequenceGap(meta) => {
                Self::resolve_sequence_gap(meta)
            }
            SyncConflict::DivergentStockState(meta) => {
                Self::resolve_divergent_stock(meta)
            }
            SyncConflict::ConflictingInventoryMutation(meta) => {
                Self::resolve_conflicting_inventory(meta)
            }
        };

        let resolution = ConflictResolutionOutcome {
            conflict: conflict.clone(),
            resolution: policy,
            audit_logged: false,
        };

        log::info!(
            target: "grpc::sync",
            "[CONFLICT_RESOLUTION] conflict_id={} type={} resolution={:?}",
            conflict.conflict_id().0,
            conflict.conflict_type_str(),
            resolution.resolution
        );

        resolution
    }

    fn resolve_duplicate_package(_meta: &ConflictMetadata) -> ResolutionPolicy {
        ResolutionPolicy::SkipIdempotent
    }

    fn resolve_stale_import(_meta: &ConflictMetadata) -> ResolutionPolicy {
        ResolutionPolicy::RejectWithAudit
    }

    fn resolve_replay_attempt(_meta: &ConflictMetadata) -> ResolutionPolicy {
        ResolutionPolicy::RejectWithAudit
    }

    fn resolve_sequence_gap(_meta: &ConflictMetadata) -> ResolutionPolicy {
        ResolutionPolicy::ManualResolutionRequired
    }

    fn resolve_divergent_stock(_meta: &ConflictMetadata) -> ResolutionPolicy {
        ResolutionPolicy::ManualResolutionRequired
    }

    fn resolve_conflicting_inventory(_meta: &ConflictMetadata) -> ResolutionPolicy {
        ResolutionPolicy::ManualResolutionRequired
    }

    pub fn classify(conflict: &SyncConflict) -> ResolutionPolicy {
        match conflict {
            SyncConflict::DuplicatePackage(_) => ResolutionPolicy::SkipIdempotent,
            SyncConflict::StaleImport(_) => ResolutionPolicy::RejectWithAudit,
            SyncConflict::ReplayAttempt(_) => ResolutionPolicy::RejectWithAudit,
            SyncConflict::SequenceGap(_) => ResolutionPolicy::ManualResolutionRequired,
            SyncConflict::DivergentStockState(_) => ResolutionPolicy::ManualResolutionRequired,
            SyncConflict::ConflictingInventoryMutation(_) => {
                ResolutionPolicy::ManualResolutionRequired
            }
        }
    }

    pub fn requires_manual_intervention(conflict: &SyncConflict) -> bool {
        matches!(
            Self::classify(conflict),
            ResolutionPolicy::ManualResolutionRequired
        )
    }
}

impl crate::architecture::Service for SyncConflictResolutionService {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::sync_integrity::types::{ConflictExplanation, ConflictId, ConflictMetadata, FiscalScope};

    fn make_meta(package_id: &str, label: &str) -> ConflictMetadata {
        ConflictMetadata {
            conflict_id: ConflictId(format!("id-{}", label)),
            package_id: package_id.into(),
            source_node_id: "node-a".into(),
            target_node_id: "node-b".into(),
            explanation: ConflictExplanation {
                label: label.into(),
                description: format!("Conflict: {}", label),
            },
            fiscal_scope: FiscalScope::unknown(),
            evidence: vec![],
        }
    }

    fn make_meta_full(
        package_id: &str,
        label: &str,
        source: &str,
        target: &str,
    ) -> ConflictMetadata {
        ConflictMetadata {
            source_node_id: source.into(),
            target_node_id: target.into(),
            ..make_meta(package_id, label)
        }
    }

    #[test]
    fn duplicate_package_skip_idempotent() {
        let conflict = SyncConflict::DuplicatePackage(make_meta("pkg-1", "Duplicate"));
        let outcome = SyncConflictResolutionService::resolve(
            &conflict, "node-a", "node-b",
        );
        assert!(matches!(
            outcome.resolution,
            ResolutionPolicy::SkipIdempotent
        ));
    }

    #[test]
    fn stale_import_rejected() {
        let conflict = SyncConflict::StaleImport(make_meta("pkg-1", "Stale"));
        let outcome = SyncConflictResolutionService::resolve(
            &conflict, "node-a", "node-b",
        );
        assert!(matches!(
            outcome.resolution,
            ResolutionPolicy::RejectWithAudit
        ));
    }

    #[test]
    fn replay_attempt_rejected() {
        let conflict = SyncConflict::ReplayAttempt(make_meta("pkg-1", "Replay"));
        let outcome = SyncConflictResolutionService::resolve(
            &conflict, "node-a", "node-b",
        );
        assert!(matches!(
            outcome.resolution,
            ResolutionPolicy::RejectWithAudit
        ));
    }

    #[test]
    fn sequence_gap_manual() {
        let conflict = SyncConflict::SequenceGap(make_meta("pkg-1", "Gap"));
        let outcome = SyncConflictResolutionService::resolve(
            &conflict, "node-a", "node-b",
        );
        assert!(matches!(
            outcome.resolution,
            ResolutionPolicy::ManualResolutionRequired
        ));
        assert!(SyncConflictResolutionService::requires_manual_intervention(
            &conflict
        ));
    }

    #[test]
    fn divergent_stock_manual() {
        let conflict = SyncConflict::DivergentStockState(make_meta("pkg-1", "Divergent"));
        assert!(SyncConflictResolutionService::requires_manual_intervention(
            &conflict
        ));
    }

    #[test]
    fn conflicting_inventory_manual() {
        let conflict =
            SyncConflict::ConflictingInventoryMutation(make_meta("pkg-1", "Conflicting"));
        assert!(SyncConflictResolutionService::requires_manual_intervention(
            &conflict
        ));
    }

    #[test]
    fn duplicate_not_manual() {
        let conflict = SyncConflict::DuplicatePackage(make_meta("pkg-1", "Duplicate"));
        assert!(!SyncConflictResolutionService::requires_manual_intervention(
            &conflict
        ));
    }

    #[test]
    fn resolve_is_deterministic() {
        let conflict = SyncConflict::StaleImport(make_meta("pkg-x", "Stale"));
        let r1 = SyncConflictResolutionService::resolve(&conflict, "node-a", "node-b");
        let r2 = SyncConflictResolutionService::resolve(&conflict, "node-a", "node-b");
        assert_eq!(
            format!("{:?}", r1.resolution),
            format!("{:?}", r2.resolution)
        );
    }

    #[test]
    fn all_variants_classified() {
        let variants = vec![
            SyncConflict::DuplicatePackage(make_meta("p1", "Dup")),
            SyncConflict::StaleImport(make_meta("p2", "Stale")),
            SyncConflict::ReplayAttempt(make_meta("p3", "Replay")),
            SyncConflict::SequenceGap(make_meta("p4", "Gap")),
            SyncConflict::DivergentStockState(make_meta("p5", "Div")),
            SyncConflict::ConflictingInventoryMutation(make_meta("p6", "Conf")),
        ];
        for v in &variants {
            let _outcome = SyncConflictResolutionService::resolve(
                v, "node-a", "node-b",
            );
        }
    }

    #[test]
    fn resolve_output_contains_conflict() {
        let conflict = SyncConflict::DuplicatePackage(make_meta_full(
            "pkg-1", "Duplicate", "node-a", "node-b",
        ));
        let outcome = SyncConflictResolutionService::resolve(
            &conflict, "node-a", "node-b",
        );
        assert_eq!(
            outcome.conflict.conflict_id().0,
            "id-Duplicate"
        );
    }

    #[test]
    fn no_silent_auto_merge_on_manual() {
        let conflict = SyncConflict::SequenceGap(make_meta("pkg-1", "Gap"));
        let outcome = SyncConflictResolutionService::resolve(
            &conflict, "node-a", "node-b",
        );
        assert!(!matches!(
            outcome.resolution,
            ResolutionPolicy::SkipIdempotent
        ));
    }
}
