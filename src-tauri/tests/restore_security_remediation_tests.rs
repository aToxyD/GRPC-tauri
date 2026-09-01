//! SEC-005 Backup/Restore remediation tests (BR-06, BR-03/BR-14, BR-05).
//!
//! Covers: restore audit survivability (external marker, boot-time
//! `AuditAction::RestoreBackup` emission, idempotency, no false-success),
//! security-regression guard + `RESTORE-OLDER-TRUST` ceremony, registry
//! overlay, crash-safety convergence, authorization matrix, and the B8
//! setup-mode interaction. SEC-057 removed the transport-sequence ledgers from
//! the restore path; only the exact-`package_id` registry overlay remains.

#[allow(dead_code)]
mod common;

use std::fs;
use std::path::{Path, PathBuf};

use grpc_lib::application::authz::Action;
use grpc_lib::application::services::runtime_bootstrap::{
    apply_pending_restore_ledger_overlays, consume_restore_markers,
};
use grpc_lib::application::services::{AuditService, RestoreRegressionStatus};
use grpc_lib::application::sync::{ImportedPackageRegistry, PackageId};
use grpc_lib::commands::backup::{
    append_restore_marker_commit, cleanup_failed_restore_artifacts, prepare_restore_backup,
    RestorePreflight,
};
use grpc_lib::commands::{authorize_command, AppState};
use grpc_lib::db::{ConnectionFactory, Database};
use grpc_lib::domain::audit::{AuditAction, AuditFilters, AuditStatus, EntityType};
use grpc_lib::domain::ports::backup::{
    restore_archive_dir, restore_ledger_sidecar_path, restore_marker_history_path, BackupPort,
    RestoreMarker,
};
use grpc_lib::errors::BusinessLogicError;
use grpc_lib::infrastructure::backup::{
    recover_interrupted_restore_and_orphans, SqliteBackupAdapter,
};
use grpc_lib::infrastructure::db::sync_import::SqliteImportedPackageRegistry;
use grpc_lib::infrastructure::security::file_encryption::AgeFileEncryptionProvider;
use grpc_lib::repositories::SyncAppliedPackagesRepository;
use tempfile::tempdir;

const ISSUER: &str = "11111111-1111-4111-8111-111111111111";
const PKG: &str = "pkg-P";
const PKG_KIND: &str = "daily_report";
// identity_store uuid fields (identity_id / subject_id / credential_id all
// parse as UUIDs at the repository boundary — see identity_store map_row).
const ANCHOR_SUBJECT: &str = "22222222-2222-4222-8222-222222222222";
const ANCHOR_CRED: &str = "33333333-3333-4333-8333-333333333333";

// ────────────────────────────── helpers ──────────────────────────────

struct Env {
    _dir: tempfile::TempDir,
    db_path: PathBuf,
    crypto: AgeFileEncryptionProvider,
}

fn env() -> Env {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("live.db");
    ConnectionFactory::new_with_path(&db_path).expect("seed db");
    Env {
        _dir: dir,
        db_path,
        crypto: AgeFileEncryptionProvider::new(),
    }
}

fn open_db(db_path: &Path) -> Database {
    ConnectionFactory::new_with_path(db_path).expect("open db")
}

fn adapter(env: &Env) -> SqliteBackupAdapter {
    SqliteBackupAdapter::new(&env.db_path, env.crypto)
}

fn create_backup(env: &Env) -> PathBuf {
    adapter(env).create_backup().expect("create backup")
}

/// Full committed restore cycle: preflight (with optional ceremony token) →
/// swap → commit line → boot phases (recovery + overlay + marker consumption).
fn committed_restore(
    env: &Env,
    backup: &Path,
    older_token: Option<&str>,
) -> (RestorePreflight, Database) {
    let preflight = prepare_restore_backup(&env.db_path, backup, older_token, "u1", env.crypto)
        .expect("preflight");
    adapter(env).restore_backup_atomic(backup).expect("swap");
    append_restore_marker_commit(&env.db_path, &preflight.marker_id).expect("commit line");
    let db = boot_phases(&env.db_path);
    (preflight, db)
}

/// Simulate the boot sequence: interrupted-restore recovery → ledger overlay →
/// normal open → marker consumption. The audit-completion signal is derived
/// from the recovery outcome (candidate became live), not from journal presence.
fn boot_phases(db_path: &Path) -> Database {
    let crypto = AgeFileEncryptionProvider::new();
    let recovery = recover_interrupted_restore_and_orphans(db_path, &crypto).expect("recovery");
    apply_pending_restore_ledger_overlays(db_path, &crypto).expect("overlay");
    let db = open_db(db_path);
    consume_restore_markers(db_path, db.executor(), recovery.restore_completed).expect("markers");
    db
}

fn restore_audit_entries(db: &Database) -> Vec<grpc_lib::domain::audit::AuditEntry> {
    AuditService::new(db.executor())
        .get_audit_entries(
            &AuditFilters {
                action: Some(AuditAction::RestoreBackup.as_str().to_string()),
                ..Default::default()
            },
            0,
            1000,
        )
        .expect("audit query")
        .entries
}

fn preflight_err(env: &Env, backup: &Path, token: Option<&str>) -> grpc_lib::errors::AppError {
    prepare_restore_backup(&env.db_path, backup, token, "u1", env.crypto)
        .err()
        .expect("expected preflight rejection")
}

fn count_restore_audits(db: &Database) -> usize {
    restore_audit_entries(db).len()
}

fn marker_ids(db: &Database) -> Vec<String> {
    restore_audit_entries(db)
        .into_iter()
        .filter_map(|e| e.entity_id)
        .collect()
}

fn read_markers(db_path: &Path) -> Vec<RestoreMarker> {
    let raw = fs::read_to_string(restore_marker_history_path(db_path)).expect("history");
    raw.lines()
        .filter_map(|l| serde_json::from_str::<RestoreMarker>(l.trim()).ok())
        .collect()
}

fn insert_ledger_row(db_path: &Path, package_id: &str, issuer: &str) {
    let db = open_db(db_path);
    let repo = SyncAppliedPackagesRepository::new(db.executor());
    repo.insert_if_new(package_id, PKG_KIND, Some(issuer), "admin", Some(issuer))
        .expect("insert package");
}

/// Insert one identity_store credential row (raw SQL — test-only).
fn insert_credential(
    db_path: &Path,
    identity_id: &str,
    subject_type: &str,
    subject_id: &str,
    credential_id: &str,
    generation: u64,
    status: &str,
) {
    let db = open_db(db_path);
    db.get_connection()
        .execute(
            "INSERT INTO identity_store
               (identity_id, subject_type, subject_id, issuer_identity_id, credential_id,
                generation, status, public_key, algorithm_version, not_after,
                created_at, updated_at, deleted)
             VALUES (?1, ?2, ?3, ?1, ?4, ?5, ?6, ?7, 1, NULL, datetime('now'), datetime('now'), 0)",
            rusqlite::params![
                identity_id,
                subject_type,
                subject_id,
                credential_id,
                generation as i64,
                status,
                vec![1u8; 32],
            ],
        )
        .expect("insert credential");
}

// ────────────────────────── BR-06: audit survivability ──────────────────────────

#[test]
fn marker_written_before_swap_and_audit_emitted_into_restored_db() {
    let e = env();
    let backup = create_backup(&e);
    let (preflight, db) = committed_restore(&e, &backup, None);

    // Marker + sidecar existed before the swap.
    assert!(restore_marker_history_path(&e.db_path).exists());
    let sidecar = restore_ledger_sidecar_path(&e.db_path, &preflight.marker_id);
    assert!(!sidecar.exists(), "sidecar must be archived after boot");
    assert!(
        restore_archive_dir(&e.db_path)
            .join(sidecar.file_name().unwrap())
            .exists(),
        "sidecar must be archived"
    );

    // The audit event lives in the RESTORED database (survives replacement)
    // and participates in the normal audit chain.
    let entries = restore_audit_entries(&db);
    assert_eq!(entries.len(), 1, "exactly one RestoreBackup audit");
    assert_eq!(
        entries[0].entity_id.as_deref(),
        Some(preflight.marker_id.as_str())
    );
    assert_eq!(entries[0].entity_type, EntityType::Backup);
    assert_eq!(entries[0].status, AuditStatus::Success);
    AuditService::new(db.executor())
        .verify_audit_hash_chain()
        .expect("audit chain valid");

    // Historical evidence retained: history line still present after boot.
    assert!(!read_markers(&e.db_path).is_empty());
}

#[test]
fn marker_write_failure_aborts_before_db_mutation() {
    let e = env();
    let backup = create_backup(&e);

    // A directory at the history path makes the marker append fail.
    fs::create_dir(restore_marker_history_path(&e.db_path)).unwrap();

    let err = preflight_err(&e, &backup, None);
    assert!(err.to_string().contains("restore history") || err.to_string().contains("سجل"));

    // No DB mutation: no journal, DB intact and openable, backup intact.
    assert!(!e.db_path.with_extension("restore.journal").exists());
    open_db(&e.db_path);
    assert!(backup.exists());

    // No orphan sidecar: the failed marker write cleans its own sidecar.
    let leftovers = fs::read_dir(e.db_path.parent().unwrap())
        .unwrap()
        .filter(|en| {
            en.as_ref()
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains(".restore.ledger.")
        })
        .count();
    assert_eq!(leftovers, 0, "no pending sidecar after failed marker write");
}

#[test]
fn repeated_boot_processing_is_idempotent_no_duplicate_audit() {
    let e = env();
    let backup = create_backup(&e);
    let (preflight, db) = committed_restore(&e, &backup, None);

    assert_eq!(count_restore_audits(&db), 1);

    // Rerun marker consumption with the same DB open (crash-after-audit sim).
    consume_restore_markers(&e.db_path, db.executor(), false).expect("rerun consume");
    assert_eq!(count_restore_audits(&db), 1, "no duplicate audit");

    // Full re-boot (crash-after-archive sim): sidecar gone → no-op.
    let db2 = boot_phases(&e.db_path);
    assert_eq!(count_restore_audits(&db2), 1);

    // A second restore cycle appends a distinct marker and its own audit.
    let backup2 = create_backup(&e);
    let (preflight2, db3) = committed_restore(&e, &backup2, None);
    assert_ne!(preflight.marker_id, preflight2.marker_id);
    let ids = marker_ids(&db3);
    assert_eq!(ids.len(), 2);
    assert!(ids.contains(&preflight.marker_id));
    assert!(ids.contains(&preflight2.marker_id));
}

#[test]
fn failed_restore_never_produces_false_success_audit() {
    let e = env();
    let backup = create_backup(&e);

    // The command writes artifacts, then the swap fails → command cleans up
    // this restore's pending-work sidecar (history line retained as evidence).
    let preflight =
        prepare_restore_backup(&e.db_path, &backup, None, "u1", e.crypto).expect("preflight");
    cleanup_failed_restore_artifacts(&e.db_path, &preflight.marker_id);
    assert!(!restore_ledger_sidecar_path(&e.db_path, &preflight.marker_id).exists());

    let db = boot_phases(&e.db_path);
    assert_eq!(
        count_restore_audits(&db),
        0,
        "no audit for a restore that never committed"
    );
    assert!(
        !read_markers(&e.db_path).is_empty(),
        "attempt evidence retained"
    );
}

#[test]
fn uncommitted_attempt_without_commit_record_gets_no_audit() {
    let e = env();
    let backup = create_backup(&e);

    // Crash between marker write and swap: no commit line, no journal.
    let preflight =
        prepare_restore_backup(&e.db_path, &backup, None, "u1", e.crypto).expect("preflight");

    let db = boot_phases(&e.db_path);
    assert_eq!(
        count_restore_audits(&db),
        0,
        "mere attempt must not be audited"
    );
    assert!(!restore_ledger_sidecar_path(&e.db_path, &preflight.marker_id).exists());
    assert!(
        !read_markers(&e.db_path).is_empty(),
        "attempt evidence retained"
    );
}

// ────────────────────── BR-03/BR-14: regression guard + ceremony ──────────────────────

#[test]
fn equal_state_backup_is_accepted_without_ceremony() {
    let e = env();
    let backup = create_backup(&e);
    let preflight =
        prepare_restore_backup(&e.db_path, &backup, None, "u1", e.crypto).expect("equal accepted");
    assert_eq!(preflight.regression, RestoreRegressionStatus::Equal);
    let markers = read_markers(&e.db_path);
    assert!(!markers[0].regressing);
}

#[test]
fn newer_state_backup_is_accepted_without_ceremony() {
    let e = env();
    let t0 = create_backup(&e); // T0: empty ledger
    insert_ledger_row(&e.db_path, PKG, ISSUER);
    let newer = create_backup(&e); // backup contains package P (newer state)
                                   // Rewind live back to T0: live is now the older state.
    adapter(&e).restore_backup_atomic(&t0).expect("rewind");

    let preflight =
        prepare_restore_backup(&e.db_path, &newer, None, "u1", e.crypto).expect("newer accepted");
    assert_eq!(preflight.regression, RestoreRegressionStatus::Newer);
    let markers = read_markers(&e.db_path);
    assert!(!markers[0].regressing);
}

#[test]
fn older_credential_generation_requires_ceremony() {
    let e = env();
    insert_credential(
        &e.db_path,
        "11111111-1111-4111-8111-111111111101",
        "WILAYA",
        ANCHOR_SUBJECT,
        ANCHOR_CRED,
        1,
        "ACTIVE",
    );
    let backup = create_backup(&e);

    // Rotate: gen1 → SUPERSEDED, gen2 ACTIVE (distinct identity rows,
    // same credential lineage, same subject — ACTIVE unique index allows it).
    {
        let db = open_db(&e.db_path);
        db.get_connection()
            .execute(
                "UPDATE identity_store SET status = 'SUPERSEDED', updated_at = datetime('now') WHERE credential_id = ?1 AND generation = 1",
                [ANCHOR_CRED],
            )
            .unwrap();
    }
    insert_credential(
        &e.db_path,
        "11111111-1111-4111-8111-111111111102",
        "WILAYA",
        ANCHOR_SUBJECT,
        ANCHOR_CRED,
        2,
        "ACTIVE",
    );

    // Without the ceremony: rejected (fail closed).
    let err = preflight_err(&e, &backup, None);
    assert!(err.to_string().contains("RESTORE-OLDER-TRUST"));

    // With the ceremony: deliberately allowed, marker records regressing=true.
    let preflight = prepare_restore_backup(
        &e.db_path,
        &backup,
        Some("RESTORE-OLDER-TRUST"),
        "u1",
        e.crypto,
    )
    .expect("ceremony allows deliberate regression");
    assert_eq!(preflight.regression, RestoreRegressionStatus::Regressing);
    let markers = read_markers(&e.db_path);
    assert!(markers.iter().any(|m| m.regressing));
}

#[test]
fn pre_revocation_backup_is_rejected_without_ceremony() {
    let e = env();
    insert_credential(
        &e.db_path,
        ISSUER,
        "WILAYA",
        ANCHOR_SUBJECT,
        ANCHOR_CRED,
        1,
        "ACTIVE",
    );
    let backup = create_backup(&e);

    // Revoke at the SAME generation (watermark 0 → 1).
    {
        let db = open_db(&e.db_path);
        db.get_connection()
            .execute(
                "UPDATE identity_store SET status = 'REVOKED', updated_at = datetime('now') WHERE credential_id = ?1",
                [ANCHOR_CRED],
            )
            .unwrap();
    }

    let err = preflight_err(&e, &backup, None);
    assert!(err.to_string().contains("RESTORE-OLDER-TRUST"));

    prepare_restore_backup(
        &e.db_path,
        &backup,
        Some("RESTORE-OLDER-TRUST"),
        "u1",
        e.crypto,
    )
    .expect("ceremony allows deliberate regression");
}

#[test]
fn wrong_ceremony_confirmation_is_rejected() {
    let e = env();
    insert_credential(
        &e.db_path,
        ISSUER,
        "WILAYA",
        ANCHOR_SUBJECT,
        ANCHOR_CRED,
        1,
        "ACTIVE",
    );
    let backup = create_backup(&e);
    {
        let db = open_db(&e.db_path);
        db.get_connection()
            .execute(
                "UPDATE identity_store SET status = 'REVOKED', updated_at = datetime('now') WHERE credential_id = ?1",
                [ANCHOR_CRED],
            )
            .unwrap();
    }

    for wrong in [
        "",
        "RESTORE",
        "restore-older-trust",
        "RESTORE-OLDER",
        "BOGUS",
        "RESTORE-OLDER-TRUSTX",
    ] {
        let err = preflight_err(&e, &backup, Some(wrong));
        assert!(
            err.to_string().contains("RESTORE-OLDER-TRUST"),
            "wrong={wrong:?}"
        );
    }
}

#[test]
fn regressing_ledger_state_requires_ceremony_and_records_marker() {
    let e = env();
    let backup = create_backup(&e); // T0: empty ledger
    insert_ledger_row(&e.db_path, PKG, ISSUER);

    let err = preflight_err(&e, &backup, None);
    assert!(err.to_string().contains("RESTORE-OLDER-TRUST"));

    let preflight = prepare_restore_backup(
        &e.db_path,
        &backup,
        Some("RESTORE-OLDER-TRUST"),
        "u1",
        e.crypto,
    )
    .expect("ceremony accepted");
    assert_eq!(preflight.regression, RestoreRegressionStatus::Regressing);

    let markers = read_markers(&e.db_path);
    let marker = markers
        .iter()
        .find(|m| m.marker_id == preflight.marker_id)
        .expect("marker recorded");
    assert!(marker.regressing);
    assert!(marker.fingerprint.registry_package_ids.is_empty());
    assert_eq!(marker.session_user_id.as_deref(), Some("u1"));
}

// ────────────────────────── BR-05: monotonic ledger overlay ──────────────────────────

#[test]
fn replay_chain_re_import_rejected_after_restore_and_overlay() {
    let e = env();
    let backup = create_backup(&e); // T0: empty registry

    // Import package P AFTER the backup was created.
    insert_ledger_row(&e.db_path, PKG, ISSUER);

    let preflight = prepare_restore_backup(
        &e.db_path,
        &backup,
        Some("RESTORE-OLDER-TRUST"),
        "u1",
        e.crypto,
    )
    .expect("ceremony");
    assert_eq!(preflight.regression, RestoreRegressionStatus::Regressing);

    // Restore T0 WITHOUT boot overlay yet: the registry is rewound.
    adapter(&e).restore_backup_atomic(&backup).expect("swap");
    {
        let db = open_db(&e.db_path);
        let repo = SyncAppliedPackagesRepository::new(db.executor());
        assert!(
            !repo.has_imported(PKG).unwrap(),
            "registry rewound before overlay"
        );
    }

    // Boot: overlay restores the registry before normal use.
    append_restore_marker_commit(&e.db_path, &preflight.marker_id).unwrap();
    let db = boot_phases(&e.db_path);

    let repo = SyncAppliedPackagesRepository::new(db.executor());
    assert!(
        repo.has_imported(PKG).unwrap(),
        "registry union preserves post-backup package"
    );

    // Re-import of P must be REJECTED (already applied).
    let registry = SqliteImportedPackageRegistry::new(
        db.executor(),
        PKG_KIND,
        Some(ISSUER),
        "admin",
        Some(ISSUER),
    );
    let err = registry
        .mark_imported(&PackageId(PKG.to_string()))
        .expect_err("re-import must be rejected");
    match err {
        grpc_lib::errors::AppError::BusinessLogic(BusinessLogicError::DuplicateSyncPackage {
            package_id,
        }) => assert_eq!(package_id, PKG),
        other => panic!("unexpected: {other:?}"),
    }
}

#[test]
fn b8_setup_mode_restore_does_not_reopen_first_import() {
    let e = env();
    // Setup-mode state: WILAYA anchor installed, registry EMPTY.
    insert_credential(
        &e.db_path,
        ISSUER,
        "WILAYA",
        ANCHOR_SUBJECT,
        ANCHOR_CRED,
        1,
        "ACTIVE",
    );
    let backup = create_backup(&e); // T0: setup state (empty registry)

    // Later the node imported its first package (registry now non-empty).
    insert_ledger_row(&e.db_path, PKG, ISSUER);

    prepare_restore_backup(
        &e.db_path,
        &backup,
        Some("RESTORE-OLDER-TRUST"),
        "u1",
        e.crypto,
    )
    .expect("ceremony");
    adapter(&e).restore_backup_atomic(&backup).expect("swap");

    // Boot overlay keeps the registry non-empty (exact package_id dedup): the
    // post-backup package is never forgotten, so its re-import stays closed.
    let crypto = AgeFileEncryptionProvider::new();
    recover_interrupted_restore_and_orphans(&e.db_path, &crypto).unwrap();
    apply_pending_restore_ledger_overlays(&e.db_path, &crypto).unwrap();
    let db = open_db(&e.db_path);

    let repo = SyncAppliedPackagesRepository::new(db.executor());
    assert!(
        repo.has_imported(PKG).unwrap(),
        "registry non-empty after setup-mode restore"
    );
}

// ────────────────────────── crash-safety convergence ──────────────────────────

#[test]
fn sidecar_without_marker_line_fails_closed() {
    let e = env();
    // Craft the state left by a crash between sidecar write and marker append.
    let orphan = restore_ledger_sidecar_path(&e.db_path, "orphan-marker");
    fs::write(&orphan, r#"{"applied_packages":[]}"#).unwrap();

    let crypto = AgeFileEncryptionProvider::new();
    apply_pending_restore_ledger_overlays(&e.db_path, &crypto).unwrap();
    let db = open_db(&e.db_path);
    let err = consume_restore_markers(&e.db_path, db.executor(), false)
        .expect_err("sidecar without marker must fail closed");
    assert!(
        err.to_string().contains("marker") || err.to_string().contains("سجل"),
        "got: {err}"
    );
    assert!(orphan.exists(), "artifact preserved for manual review");
}

#[test]
fn interrupted_swap_journal_cases_converge_with_audit() {
    // Case A: journal written, live still in place, candidate present.
    let e = env();
    let backup = create_backup(&e);
    let candidate = e.db_path.with_file_name("candidate_a.db");
    ConnectionFactory::new_with_path(&candidate).unwrap();
    let _preflight =
        prepare_restore_backup(&e.db_path, &backup, None, "u1", e.crypto).expect("preflight");
    let journal = e.db_path.with_extension("restore.journal");
    fs::write(
        &journal,
        serde_json::json!({
            "version": 1,
            "phase": "ready_swap",
            "candidate_path": candidate,
            "rollback_path": e.db_path.with_file_name("rollback_a.db"),
        })
        .to_string(),
    )
    .unwrap();

    let db = boot_phases(&e.db_path); // recovery completes the swap → audit
    assert_eq!(
        count_restore_audits(&db),
        1,
        "interrupted swap completes and is audited"
    );
    assert!(!journal.exists());

    // Case B: live already moved to rollback, candidate present.
    let e = env();
    let backup = create_backup(&e);
    let candidate = e.db_path.with_file_name("candidate_b.db");
    ConnectionFactory::new_with_path(&candidate).unwrap();
    let _preflight =
        prepare_restore_backup(&e.db_path, &backup, None, "u1", e.crypto).expect("preflight");
    let journal = e.db_path.with_extension("restore.journal");
    let rollback = e.db_path.with_file_name("rollback_b.db");
    fs::rename(&e.db_path, &rollback).unwrap(); // crash after live→rollback rename
    fs::write(
        &journal,
        serde_json::json!({
            "version": 1,
            "phase": "ready_swap",
            "candidate_path": candidate,
            "rollback_path": rollback,
        })
        .to_string(),
    )
    .unwrap();

    let db = boot_phases(&e.db_path);
    assert!(e.db_path.exists());
    assert_eq!(count_restore_audits(&db), 1);

    // Case C: swap finished (candidate → live), rollback + journal remain.
    let e = env();
    let backup = create_backup(&e);
    let candidate = e.db_path.with_file_name("candidate_c.db");
    ConnectionFactory::new_with_path(&candidate).unwrap();
    let _preflight =
        prepare_restore_backup(&e.db_path, &backup, None, "u1", e.crypto).expect("preflight");
    let journal = e.db_path.with_extension("restore.journal");
    let rollback = e.db_path.with_file_name("rollback_c.db");
    fs::write(&rollback, b"old db").unwrap(); // stale rollback
    fs::write(
        &journal,
        serde_json::json!({
            "version": 1,
            "phase": "ready_swap",
            "candidate_path": candidate,
            "rollback_path": rollback,
        })
        .to_string(),
    )
    .unwrap();

    let db = boot_phases(&e.db_path);
    assert!(!rollback.exists(), "stale rollback removed");
    assert_eq!(count_restore_audits(&db), 1);
}

#[test]
fn crash_after_audit_before_archive_converges() {
    let e = env();
    let backup = create_backup(&e);
    let (_preflight, db) = committed_restore(&e, &backup, None);
    assert_eq!(count_restore_audits(&db), 1);

    // Simulate crash-after-audit-before-archive by blocking the archive step:
    // a regular FILE at the archive dir path breaks create_dir_all + rename.
    // (The prior boot already archived; re-create the state for this test by
    // writing a fresh sidecar + marker line directly.)
    let sidecar = restore_ledger_sidecar_path(&e.db_path, "dup-marker");
    fs::write(&sidecar, r#"{"applied_packages":[]}"#).unwrap();
    let history = restore_marker_history_path(&e.db_path);
    let marker_json = serde_json::json!({
        "marker_id": "dup-marker",
        "timestamp": "2026-08-17T00:00:00Z",
        "backup_path": "/tmp/dup.bak",
        "session_user_id": "u1",
        "fingerprint": {
            "credential_states": [],
            "active_wilaya_anchor_generation": null,
            "registry_package_ids": []
        },
        "regressing": false
    })
    .to_string();
    fs::write(&history, format!("{marker_json}\n")).unwrap();
    append_restore_marker_commit(&e.db_path, "dup-marker").unwrap();

    let archive_dir = restore_archive_dir(&e.db_path);
    fs::remove_dir_all(&archive_dir).unwrap();
    fs::write(&archive_dir, b"blocking file").unwrap();

    let err = consume_restore_markers(&e.db_path, db.executor(), false)
        .expect_err("archive failure must fail closed");
    assert!(err.to_string().contains("أرشيف"), "got: {err}");
    // The audit event for dup-marker was emitted BEFORE the archive step
    // failed (1 original + 1 dup) — the event is durable even if archiving
    // is obstructed.
    assert_eq!(count_restore_audits(&db), 2, "audit already emitted");
    assert!(sidecar.exists(), "sidecar retained for retry");

    // Operator clears the obstruction → rerun converges: dedup + archive.
    fs::remove_file(&archive_dir).unwrap();
    consume_restore_markers(&e.db_path, db.executor(), false).expect("converged rerun");
    assert_eq!(
        count_restore_audits(&db),
        2,
        "no duplicate audit after convergence"
    );
    assert!(!sidecar.exists(), "sidecar archived after convergence");
}

// ────────────────────────── residual LOW: no audit without completion ──────────────────────────

/// Window 1 (residual LOW): the swap FAILED — the old DB was restored — but the
/// process crashed inside the failure handler before the journal was cleared.
/// The journal survives while the old DB is live. Boot recovery must report
/// `restore_completed = false` and MUST NOT emit a RestoreBackup audit.
#[test]
fn failed_swap_with_surviving_journal_produces_no_audit() {
    let e = env();
    let backup = create_backup(&e);
    let _preflight =
        prepare_restore_backup(&e.db_path, &backup, None, "u1", e.crypto).expect("preflight");

    // Failed-swap handler state: rollback already renamed back to live, then
    // candidate removed — crash before clear_journal. Journal lists paths that
    // no longer exist; the old DB is live.
    let journal = e.db_path.with_extension("restore.journal");
    fs::write(
        &journal,
        serde_json::json!({
            "version": 1,
            "phase": "ready_swap",
            "candidate_path": e.db_path.with_file_name("gone_candidate.db"),
            "rollback_path": e.db_path.with_file_name("gone_rollback.db"),
        })
        .to_string(),
    )
    .unwrap();

    let db = boot_phases(&e.db_path);
    assert!(e.db_path.exists(), "old DB remains live");
    assert!(!journal.exists(), "journal cleared after recovery");
    assert_eq!(
        count_restore_audits(&db),
        0,
        "failed restore must not produce a RestoreBackup audit"
    );
}

/// Case E (residual LOW variant): the candidate is lost and the rollback
/// (old DB) is restored to live. The restore did NOT complete — no audit.
#[test]
fn case_e_rollback_restored_produces_no_audit() {
    let e = env();
    let backup = create_backup(&e);
    let _preflight =
        prepare_restore_backup(&e.db_path, &backup, None, "u1", e.crypto).expect("preflight");

    // Live already moved to rollback (swap in progress), candidate consumed by
    // the failed-promotion handler, journal survives.
    let rollback = e.db_path.with_file_name("rollback_e.db");
    fs::rename(&e.db_path, &rollback).unwrap();
    let journal = e.db_path.with_extension("restore.journal");
    fs::write(
        &journal,
        serde_json::json!({
            "version": 1,
            "phase": "ready_swap",
            "candidate_path": e.db_path.with_file_name("consumed_candidate.db"),
            "rollback_path": rollback,
        })
        .to_string(),
    )
    .unwrap();

    let db = boot_phases(&e.db_path);
    assert!(e.db_path.exists(), "old DB restored from rollback");
    assert!(!rollback.exists(), "rollback consumed");
    assert!(!journal.exists());
    assert_eq!(
        count_restore_audits(&db),
        0,
        "old DB is live — the restore never took effect, no audit"
    );
}

/// Recovery outcome semantics: completion reported ONLY when the candidate
/// actually became the live database.
#[test]
fn recovery_outcome_reports_completion() {
    let crypto = AgeFileEncryptionProvider::new();

    // No journal → nothing to complete.
    let e = env();
    let outcome = recover_interrupted_restore_and_orphans(&e.db_path, &crypto).unwrap();
    assert!(!outcome.restore_completed);

    // Case A: journal + candidate + live → full swap → completed.
    let e = env();
    let candidate = e.db_path.with_file_name("cand_a.db");
    fs::write(&candidate, b"x").unwrap();
    fs::write(
        e.db_path.with_extension("restore.journal"),
        serde_json::json!({
            "version": 1,
            "phase": "ready_swap",
            "candidate_path": candidate,
            "rollback_path": e.db_path.with_file_name("rb_a.db"),
        })
        .to_string(),
    )
    .unwrap();
    let outcome = recover_interrupted_restore_and_orphans(&e.db_path, &crypto).unwrap();
    assert!(outcome.restore_completed, "Case A: candidate became live");

    // Case C: candidate already promoted, rollback + journal remain → completed.
    let e = env();
    let rollback = e.db_path.with_file_name("rb_c.db");
    fs::write(&rollback, b"old").unwrap();
    fs::write(
        e.db_path.with_extension("restore.journal"),
        serde_json::json!({
            "version": 1,
            "phase": "ready_swap",
            "candidate_path": e.db_path.with_file_name("gone_c.db"),
            "rollback_path": rollback,
        })
        .to_string(),
    )
    .unwrap();
    let outcome = recover_interrupted_restore_and_orphans(&e.db_path, &crypto).unwrap();
    assert!(outcome.restore_completed, "Case C: candidate is live");

    // Window 1: journal survives a FAILED swap → NOT completed.
    let e = env();
    fs::write(
        e.db_path.with_extension("restore.journal"),
        serde_json::json!({
            "version": 1,
            "phase": "ready_swap",
            "candidate_path": e.db_path.with_file_name("gone1.db"),
            "rollback_path": e.db_path.with_file_name("gone2.db"),
        })
        .to_string(),
    )
    .unwrap();
    let outcome = recover_interrupted_restore_and_orphans(&e.db_path, &crypto).unwrap();
    assert!(!outcome.restore_completed, "failed swap: old DB live");

    // Corrupt journal → NOT completed (and removed).
    let e = env();
    fs::write(e.db_path.with_extension("restore.journal"), b"{}").unwrap();
    let outcome = recover_interrupted_restore_and_orphans(&e.db_path, &crypto).unwrap();
    assert!(
        !outcome.restore_completed,
        "corrupt journal is not completion"
    );
    assert!(!e.db_path.with_extension("restore.journal").exists());
}

// ────────────────────────── authorization matrix ──────────────────────────

fn set_session(state: &AppState, session: grpc_lib::domain::session::CurrentSession) {
    common::insert_test_user(
        state,
        &session.user_id,
        &session.username,
        &session.user_role.to_string(),
    );
    state.current_session.lock().unwrap().replace(session);
}

fn unit_state() -> AppState {
    let db = grpc_lib::db::ConnectionFactory::new_for_test().expect("db");
    let state = AppState::new_for_test(db);
    {
        let guard = state.get_db().expect("lock");
        let db = guard.as_ref().expect("db");
        db.get_connection()
            .execute(
                "UPDATE settings SET node_type = 'UNIT', configured = 1, unit_name = 'unit-16', wilaya_code = '16' WHERE id = 1",
                [],
            )
            .expect("settings");
    }
    state
}

fn wilaya_state() -> AppState {
    let db = grpc_lib::db::ConnectionFactory::new_for_test().expect("db");
    let state = AppState::new_for_test(db);
    {
        let guard = state.get_db().expect("lock");
        let db = guard.as_ref().expect("db");
        db.get_connection()
            .execute(
                "UPDATE settings SET node_type = 'WILAYA', configured = 1, wilaya_code = '16', wilaya_name = 'W' WHERE id = 1",
                [],
            )
            .expect("settings");
    }
    state
}

#[test]
fn authorization_matrix_backup_restore() {
    // UNIT User: may create/list backups, must NOT restore.
    let state = unit_state();
    set_session(&state, common::create_test_session("u1", "user1", "User"));
    authorize_command(&state, Action::ManageBackups, None).expect("UNIT User may manage backups");
    let err = authorize_command(&state, Action::AdminOnly, None)
        .expect_err("UNIT User must be denied restore");
    assert!(matches!(
        err,
        grpc_lib::errors::AppError::Authorization(
            grpc_lib::errors::AuthorizationError::RequiresAdmin
        )
    ));

    // UNIT Admin: full access.
    let state = unit_state();
    set_session(&state, common::create_test_session("a1", "admin1", "Admin"));
    authorize_command(&state, Action::ManageBackups, None).expect("UNIT Admin backups");
    authorize_command(&state, Action::AdminOnly, None).expect("UNIT Admin restore");

    // WILAYA Admin: full access.
    let state = wilaya_state();
    set_session(&state, common::create_test_session("a2", "admin2", "Admin"));
    authorize_command(&state, Action::ManageBackups, None).expect("WILAYA Admin backups");
    authorize_command(&state, Action::AdminOnly, None).expect("WILAYA Admin restore");
}

#[test]
fn ceremony_token_grants_no_authority_to_user() {
    // A User supplying the ceremony token still fails the AdminOnly gate —
    // the ceremony is an additional condition for an already-authorized Admin,
    // never an authorization mechanism.
    let state = unit_state();
    set_session(&state, common::create_test_session("u1", "user1", "User"));
    for _token in ["RESTORE-OLDER-TRUST", "RESTORE"] {
        let err = authorize_command(&state, Action::AdminOnly, None)
            .expect_err("token must not bypass AdminOnly");
        assert!(matches!(
            err,
            grpc_lib::errors::AppError::Authorization(
                grpc_lib::errors::AuthorizationError::RequiresAdmin
            )
        ));
    }

    // And the internal preflight itself is not an IPC command at all — a User
    // cannot reach it through any command surface.
    assert!(grpc_lib::commands::backup::prepare_restore_backup(
        Path::new("/nonexistent/live.db"),
        Path::new("/nonexistent/b.bak"),
        None,
        "u1",
        AgeFileEncryptionProvider::new(),
    )
    .is_err());
}

// ────────────────────────── XB-B: stale ADMIN resurrection ──────────────────────────

const ADMIN_CRED: &str = "ADMIN-CRED-0000000000000000001";

/// XB-B: a backup taken before the node's ADMIN credential existed must not
/// silently restore a `NoActiveAdmin` state — the ADMIN authentication gate
/// would reopen with obsolete/no credentials. Ceremony required, fail closed.
#[test]
fn pre_admin_backup_is_rejected_once_admin_credential_exists() {
    let e = env();
    let pre_admin = create_backup(&e); // candidate predates the ADMIN credential

    // The live node now has an authoritative ACTIVE ADMIN credential.
    insert_credential(
        &e.db_path,
        "22222222-2222-4222-8222-222222222201",
        "ADMIN",
        "admin-subject-1",
        ADMIN_CRED,
        1,
        "ACTIVE",
    );

    let err = preflight_err(&e, &pre_admin, None);
    assert!(
        err.to_string().contains("RESTORE-OLDER-TRUST"),
        "missing live ADMIN credential must require the ceremony"
    );

    let preflight = prepare_restore_backup(
        &e.db_path,
        &pre_admin,
        Some("RESTORE-OLDER-TRUST"),
        "u1",
        e.crypto,
    )
    .expect("ceremony allows deliberate regression");
    assert_eq!(preflight.regression, RestoreRegressionStatus::Regressing);
    let markers = read_markers(&e.db_path);
    assert!(
        markers.iter().any(|m| m.regressing),
        "marker must record the deliberate regression"
    );
}

/// XB-B: a candidate carrying an older ADMIN generation must not silently
/// downgrade the ADMIN credential — ceremony required.
#[test]
fn stale_admin_generation_backup_is_rejected_without_ceremony() {
    let e = env();
    insert_credential(
        &e.db_path,
        "22222222-2222-4222-8222-222222222202",
        "ADMIN",
        "admin-subject-1",
        ADMIN_CRED,
        1,
        "ACTIVE",
    );
    let backup = create_backup(&e); // candidate holds ADMIN generation 1

    // Rotate the ADMIN credential to generation 2 (same credential lineage).
    {
        let db = open_db(&e.db_path);
        db.get_connection()
            .execute(
                "UPDATE identity_store SET status = 'SUPERSEDED', updated_at = datetime('now') WHERE credential_id = ?1 AND generation = 1",
                [ADMIN_CRED],
            )
            .unwrap();
    }
    insert_credential(
        &e.db_path,
        "22222222-2222-4222-8222-222222222203",
        "ADMIN",
        "admin-subject-1",
        ADMIN_CRED,
        2,
        "ACTIVE",
    );

    let err = preflight_err(&e, &backup, None);
    assert!(err.to_string().contains("RESTORE-OLDER-TRUST"));

    prepare_restore_backup(
        &e.db_path,
        &backup,
        Some("RESTORE-OLDER-TRUST"),
        "u1",
        e.crypto,
    )
    .expect("ceremony allows deliberate regression");
}

/// XB-B: a backup taken while the ADMIN credential is ACTIVE must not force
/// the ceremony — the dimension must only protect the live ADMIN state.
#[test]
fn admin_credential_in_backup_does_not_force_ceremony() {
    let e = env();
    insert_credential(
        &e.db_path,
        "22222222-2222-4222-8222-222222222204",
        "ADMIN",
        "admin-subject-1",
        ADMIN_CRED,
        1,
        "ACTIVE",
    );
    let backup = create_backup(&e);

    let preflight = prepare_restore_backup(&e.db_path, &backup, None, "u1", e.crypto)
        .expect("equal admin state accepted without ceremony");
    assert_eq!(preflight.regression, RestoreRegressionStatus::Equal);
    let markers = read_markers(&e.db_path);
    assert!(!markers[0].regressing);
}
