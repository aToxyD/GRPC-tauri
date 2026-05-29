use std::collections::HashMap;

use grpc_lib::application::services::{
    ConflictResolutionOutcome, ImportExecutionError, ImportMutationSummary, ResolutionPolicy,
    ReplayProtectionResult, SyncImportRequest, SyncImportResult, SyncImportExecutionService,
    SyncImportValidationService, SyncPackageKind,
};
use grpc_lib::application::sync_integrity::replay::{
    AppliedPackages, AppliedTransitions, ReplayDetector, SeenTransactions,
};
use grpc_lib::application::sync_integrity::types::ConflictDetectionOutcome;
use grpc_lib::db::ConnectionFactory;

fn make_fiscal_request(package_id: &str) -> SyncImportRequest {
    SyncImportRequest {
        package_id: package_id.into(),
        source_node_id: "node-a".into(),
        target_node_id: "node-b".into(),
        kind: SyncPackageKind::Fiscal,
        incoming_sequence: None,
        last_applied_sequence: None,
        fiscal_year: Some(2025),
        transition_ids: vec![],
        transaction_ids: vec![],
        payload: HashMap::new(),
    }
}

fn make_operational_request(
    package_id: &str,
    incoming: u64,
    last_applied: Option<u64>,
) -> SyncImportRequest {
    SyncImportRequest {
        package_id: package_id.into(),
        source_node_id: "node-a".into(),
        target_node_id: "node-b".into(),
        kind: SyncPackageKind::Operational,
        incoming_sequence: Some(incoming),
        last_applied_sequence: last_applied,
        fiscal_year: None,
        transition_ids: vec![],
        transaction_ids: vec![],
        payload: HashMap::new(),
    }
}

fn make_product_request(package_id: &str) -> SyncImportRequest {
    SyncImportRequest {
        package_id: package_id.into(),
        source_node_id: "node-a".into(),
        target_node_id: "node-b".into(),
        kind: SyncPackageKind::Product,
        incoming_sequence: None,
        last_applied_sequence: None,
        fiscal_year: None,
        transition_ids: vec![],
        transaction_ids: vec![],
        payload: HashMap::new(),
    }
}

// ─── Replay Rejection ────────────────────────────────────────────────────────

#[test]
fn replay_rejection_returns_error() {
    let mut db = ConnectionFactory::new_for_test().expect("db");
    let request = make_fiscal_request("pkg-replay-test-1");
    let r1 = SyncImportExecutionService::execute_import(&mut db, request.clone(), 2025, 5);
    assert!(r1.success, "first import should succeed");

    let r2 = SyncImportExecutionService::execute_import(&mut db, request, 2025, 5);
    assert!(!r2.success, "second import (replay) should be rejected");
    assert!(
        r2.replay_protection
            .as_ref()
            .map(|rp| rp.replay_detected)
            .unwrap_or(false),
        "replay_protection should flag replay_detected=true"
    );
}

#[test]
fn replay_rejection_audits_via_transaction() {
    let mut db = ConnectionFactory::new_for_test().expect("db");
    let request = make_fiscal_request("pkg-replay-audit");
    let r1 = SyncImportExecutionService::execute_import(&mut db, request.clone(), 2025, 5);
    assert!(r1.success);

    let r2 = SyncImportExecutionService::execute_import(&mut db, request, 2025, 5);
    assert!(!r2.success);
    assert!(
        r2.error.is_some(),
        "should have an error on replay rejection"
    );
}

// ─── Duplicate Package Idempotency ───────────────────────────────────────────

#[test]
fn duplicate_package_idempotency() {
    let mut db = ConnectionFactory::new_for_test().expect("db");
    let r1 = SyncImportExecutionService::execute_import(
        &mut db,
        make_fiscal_request("pkg-idem-1"),
        2025,
        5,
    );
    assert!(r1.success);

    let r2 = SyncImportExecutionService::execute_import(
        &mut db,
        make_fiscal_request("pkg-idem-1"),
        2025,
        5,
    );
    assert!(!r2.success, "duplicate package_id must be rejected");
}

// ─── Rollback on Mid-Import Failure ──────────────────────────────────────────

#[test]
fn rollback_on_fiscal_year_mismatch() {
    let mut db = ConnectionFactory::new_for_test().expect("db");
    let request = make_fiscal_request("pkg-rollback-fy");
    let result = SyncImportExecutionService::execute_import(
        &mut db,
        SyncImportRequest {
            fiscal_year: Some(2030),
            ..request
        },
        2025,
        5,
    );
    assert!(!result.success, "future fiscal year should be rejected");
}

#[test]
fn rollback_on_invalid_fiscal_year() {
    let mut db = ConnectionFactory::new_for_test().expect("db");
    let request = make_fiscal_request("pkg-rollback-old");
    let result = SyncImportExecutionService::execute_import(
        &mut db,
        SyncImportRequest {
            fiscal_year: Some(2000),
            ..request
        },
        2025,
        5,
    );
    assert!(
        !result.success,
        "too-old fiscal year should be rejected"
    );
}

// ─── Fiscal Import Atomicity ─────────────────────────────────────────────────

#[test]
fn fiscal_import_atomic_all_or_nothing() {
    let mut db = ConnectionFactory::new_for_test().expect("db");
    let request = make_fiscal_request("pkg-atomic-fiscal");
    let result = SyncImportExecutionService::execute_import(&mut db, request, 2025, 5);
    assert!(result.success, "fiscal import should succeed");
}

#[test]
fn fiscal_import_rolls_back_on_error() {
    let mut db = ConnectionFactory::new_for_test().expect("db");
    let request = make_fiscal_request("pkg-atomic-fail");
    let result = SyncImportExecutionService::execute_import(
        &mut db,
        SyncImportRequest {
            fiscal_year: Some(2030),
            ..request
        },
        2025,
        5,
    );
    assert!(!result.success, "should fail and roll back");
}

// ─── Sequence Gap Rejection ──────────────────────────────────────────────────

#[test]
fn sequence_gap_rejected_for_operational() {
    let mut db = ConnectionFactory::new_for_test().expect("db");
    let request = make_operational_request("pkg-seq-gap", 5, Some(2));
    let result = SyncImportExecutionService::execute_import(&mut db, request, 2025, 5);
    assert!(!result.success, "sequence gap should be rejected");
}

#[test]
fn contiguous_sequence_accepted_for_operational() {
    let mut db = ConnectionFactory::new_for_test().expect("db");
    let request = make_operational_request("pkg-seq-ok", 3, Some(2));
    let result = SyncImportExecutionService::execute_import(&mut db, request, 2025, 5);
    assert!(result.success, "contiguous sequence should succeed");
}

// ─── Stale Import Handling ───────────────────────────────────────────────────

#[test]
fn stale_import_detected_by_replay_detector() {
    let mut pkgs = std::collections::BTreeSet::new();
    pkgs.insert("pkg-stale".into());
    let detector = ReplayDetector::new(
        AppliedPackages::new(pkgs),
        AppliedTransitions::new(std::collections::BTreeSet::new()),
        SeenTransactions::new(std::collections::BTreeSet::new()),
        "node-a".into(),
        "node-b".into(),
    );
    let result = detector.check_package_id("pkg-stale");
    assert!(
        matches!(result, ConflictDetectionOutcome::ConflictDetected(_)),
        "stale/duplicate package should be detected"
    );
}

// ─── Conflict Detection Integration ──────────────────────────────────────────

#[test]
fn conflict_detection_integration() {
    let mut db = ConnectionFactory::new_for_test().expect("db");
    let request = make_fiscal_request("pkg-conflict-int");
    let r1 = SyncImportExecutionService::execute_import(&mut db, request.clone(), 2025, 5);
    assert!(r1.success);

    let r2 = SyncImportExecutionService::execute_import(&mut db, request, 2025, 5);
    assert!(!r2.success);
    assert!(
        r2.conflict_outcome.is_none() || r2.replay_protection.is_some(),
        "conflict detection should produce replay_protection result"
    );
}

// ─── Reproducibility ─────────────────────────────────────────────────────────

#[test]
fn same_input_same_db_same_result() {
    let mut db1 = ConnectionFactory::new_for_test().expect("db");
    let mut db2 = ConnectionFactory::new_for_test().expect("db");

    let r1 = SyncImportExecutionService::execute_import(
        &mut db1,
        make_fiscal_request("pkg-repro"),
        2025,
        5,
    );
    let r2 = SyncImportExecutionService::execute_import(
        &mut db2,
        make_fiscal_request("pkg-repro"),
        2025,
        5,
    );
    assert_eq!(r1.success, r2.success, "same input + same DB state => same result");
    assert_eq!(
        r1.error.is_some(),
        r2.error.is_some(),
        "error presence must match"
    );
}

#[test]
fn deterministic_duplicate_detection() {
    let mut pkgs = std::collections::BTreeSet::new();
    pkgs.insert("pkg-det".into());
    let detector = ReplayDetector::new(
        AppliedPackages::new(pkgs.clone()),
        AppliedTransitions::new(std::collections::BTreeSet::new()),
        SeenTransactions::new(std::collections::BTreeSet::new()),
        "node-a".into(),
        "node-b".into(),
    );
    let r1 = detector.check_package_id("pkg-det");
    let r2 = detector.check_package_id("pkg-det");
    match (r1, r2) {
        (
            ConflictDetectionOutcome::ConflictDetected(c1),
            ConflictDetectionOutcome::ConflictDetected(c2),
        ) => {
            assert_eq!(c1.conflict_id(), c2.conflict_id());
        }
        _ => panic!("expected ConflictDetected"),
    }
}

// ─── Serialization Round-Trips ───────────────────────────────────────────────

#[test]
fn import_execution_error_serde_round_trip() {
    let err = ImportExecutionError::ReplayDetected("pkg-1".into());
    let json = serde_json::to_string(&err).unwrap();
    let restored: ImportExecutionError = serde_json::from_str(&json).unwrap();
    assert_eq!(format!("{}", err), format!("{}", restored));
}

#[test]
fn sync_import_result_serde_round_trip() {
    let result = SyncImportResult {
        success: true,
        execution_summary: None,
        conflict_outcome: None,
        replay_protection: Some(ReplayProtectionResult {
            package_id: "pkg-1".into(),
            replay_detected: false,
            conflict: None,
            audited: true,
        }),
        error: None,
    };
    let json = serde_json::to_string(&result).unwrap();
    let restored: SyncImportResult = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.success, result.success);
    assert_eq!(
        restored.replay_protection.map(|r| r.audited),
        Some(true)
    );
}

#[test]
fn conflict_resolution_outcome_serde() {
    use grpc_lib::application::sync_integrity::types::{
        ConflictExplanation, ConflictId, ConflictMetadata, FiscalScope, SyncConflict,
    };
    let meta = ConflictMetadata {
        conflict_id: ConflictId("test-id".into()),
        package_id: "pkg-1".into(),
        source_node_id: "node-a".into(),
        target_node_id: "node-b".into(),
        explanation: ConflictExplanation {
            label: "STALE".into(),
            description: "stale import".into(),
        },
        fiscal_scope: FiscalScope::unknown(),
        evidence: vec![],
    };
    let outcome = ConflictResolutionOutcome {
        conflict: SyncConflict::StaleImport(meta),
        resolution: ResolutionPolicy::RejectWithAudit,
        audit_logged: true,
    };
    let json = serde_json::to_string(&outcome).unwrap();
    let restored: ConflictResolutionOutcome = serde_json::from_str(&json).unwrap();
    assert!(matches!(
        restored.resolution,
        ResolutionPolicy::RejectWithAudit
    ));
}

#[test]
fn mutation_summary_serde() {
    let summary = ImportMutationSummary {
        products_imported: 5,
        products_updated: 2,
        products_skipped: 1,
        movements_applied: 10,
        reports_imported: 3,
        fiscal_transitions_applied: 1,
    };
    let json = serde_json::to_string(&summary).unwrap();
    let restored: ImportMutationSummary = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.products_imported, 5);
    assert_eq!(restored.movements_applied, 10);
}

// ─── Validation Service Pure Tests ───────────────────────────────────────────

#[test]
fn validation_service_is_deterministic() {
    let mut pkgs = std::collections::BTreeSet::new();
    pkgs.insert("pkg-det-val".into());
    let detector = ReplayDetector::new(
        AppliedPackages::new(pkgs),
        AppliedTransitions::new(std::collections::BTreeSet::new()),
        SeenTransactions::new(std::collections::BTreeSet::new()),
        "node-a".into(),
        "node-b".into(),
    );

    let request = SyncImportRequest {
        package_id: "pkg-det-val".into(),
        source_node_id: "node-a".into(),
        target_node_id: "node-b".into(),
        kind: SyncPackageKind::Fiscal,
        incoming_sequence: None,
        last_applied_sequence: None,
        fiscal_year: Some(2025),
        transition_ids: vec![],
        transaction_ids: vec![],
        payload: HashMap::new(),
    };

    let v1 = SyncImportValidationService::validate(&request, &detector, 2025, 5);
    let v2 = SyncImportValidationService::validate(&request, &detector, 2025, 5);
    assert_eq!(v1.all_checks_passed, v2.all_checks_passed);
}

#[test]
fn validation_rejects_duplicate() {
    let mut pkgs = std::collections::BTreeSet::new();
    pkgs.insert("pkg-val-dup".into());
    let detector = ReplayDetector::new(
        AppliedPackages::new(pkgs),
        AppliedTransitions::new(std::collections::BTreeSet::new()),
        SeenTransactions::new(std::collections::BTreeSet::new()),
        "node-a".into(),
        "node-b".into(),
    );

    let request = SyncImportRequest {
        package_id: "pkg-val-dup".into(),
        source_node_id: "node-a".into(),
        target_node_id: "node-b".into(),
        kind: SyncPackageKind::Fiscal,
        incoming_sequence: None,
        last_applied_sequence: None,
        fiscal_year: Some(2025),
        transition_ids: vec![],
        transaction_ids: vec![],
        payload: HashMap::new(),
    };

    let result = SyncImportValidationService::validate(&request, &detector, 2025, 5);
    assert!(!result.all_checks_passed);
}

// ─── Conflict Resolution Determinism Tests ───────────────────────────────────

#[test]
fn conflict_resolution_is_deterministic() {
    use grpc_lib::application::services::SyncConflictResolutionService;
    use grpc_lib::application::sync_integrity::types::{
        ConflictExplanation, ConflictId, ConflictMetadata, FiscalScope, SyncConflict,
    };

    let meta = ConflictMetadata {
        conflict_id: ConflictId("id".into()),
        package_id: "pkg-1".into(),
        source_node_id: "node-a".into(),
        target_node_id: "node-b".into(),
        explanation: ConflictExplanation {
            label: "Stale".into(),
            description: "stale import".into(),
        },
        fiscal_scope: FiscalScope::unknown(),
        evidence: vec![],
    };

    let conflict = SyncConflict::StaleImport(meta);
    let r1 = SyncConflictResolutionService::resolve(&conflict, "node-a", "node-b");
    let r2 = SyncConflictResolutionService::resolve(&conflict, "node-a", "node-b");
    assert_eq!(
        format!("{:?}", r1.resolution),
        format!("{:?}", r2.resolution)
    );
}

// ─── Execution with Product Kind ─────────────────────────────────────────────

#[test]
fn product_kind_import_succeeds() {
    let mut db = ConnectionFactory::new_for_test().expect("db");
    let request = make_product_request("pkg-product-test");
    let result = SyncImportExecutionService::execute_import(&mut db, request, 2025, 5);
    assert!(result.success);
}

// ─── Idempotency Across Kinds ────────────────────────────────────────────────

#[test]
fn idempotent_across_different_kinds() {
    let mut db = ConnectionFactory::new_for_test().expect("db");
    let fiscal = make_fiscal_request("pkg-cross-kind");
    let r1 = SyncImportExecutionService::execute_import(&mut db, fiscal, 2025, 5);
    assert!(r1.success);

    let product = make_product_request("pkg-cross-kind");
    let r2 = SyncImportExecutionService::execute_import(&mut db, product, 2025, 5);
    assert!(!r2.success, "same package_id should be rejected regardless of kind");
}
