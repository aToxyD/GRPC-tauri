use std::io;
use std::path::{Path, PathBuf};

/// Backup information
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BackupInfo {
    pub filename: String,
    pub path: String,
    pub size_bytes: u64,
    pub created: String,
}

/// Monotonic security-state fingerprint of a database (SEC-005 BR-03/BR-14).
///
/// Captures only the monotonic security state that must never silently regress:
/// credential generations + terminal-status watermark, the ACTIVE WILAYA trust
/// anchor, the imported-package registry, per-issuer replay maxima, and the
/// per-`(issuer, target)` producer transport maxima. Account fields
/// (deleted/role/enabled) are intentionally excluded — they are mutable by
/// design.
///
/// XB-B: the node's authoritative ACTIVE ADMIN credential is captured
/// explicitly so a restore can never silently resurrect a pre-ADMIN /
/// stale-admin authentication state (password gate re-opened via
/// `NoActiveAdmin`). The same fact also appears in `credential_states`; the
/// dedicated dimension makes the invariant explicit and immune to future
/// changes of that collection.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SecurityFingerprint {
    /// `(credential_id, max generation, status watermark)` for every
    /// non-deleted credential, sorted by credential_id. The watermark is the
    /// maximum severity ever reached by the credential: 0 = ACTIVE-only,
    /// 1 = REVOKED/SUPERSEDED/EXPIRED. It makes pre-revocation/pre-rotation
    /// backups detectable even when the generation is unchanged.
    pub credential_states: Vec<(String, u64, u64)>,
    /// Generation of the ACTIVE WILAYA trust anchor, if installed.
    pub active_wilaya_anchor_generation: Option<u64>,
    /// All accepted imported package ids, sorted.
    pub registry_package_ids: Vec<String>,
    /// `(issuer_identity_id, last applied sequence)` for every issuer, sorted by issuer.
    pub issuer_sequences: Vec<(String, u64)>,
    /// The live node's ACTIVE ADMIN credential `(credential_id, generation)`,
    /// when one exists (XB-B). `#[serde(default)]` keeps markers written by
    /// earlier versions deserializable.
    #[serde(default)]
    pub active_admin_credential: Option<(String, u64)>,
    /// `(issuer_identity_id, target_node_id, last issued sequence)` for every
    /// producer stream, sorted by `(issuer, target)` (SEC-054, F1). Added so a
    /// restore can never silently rewind the producer transport stream, which
    /// would otherwise allow a duplicate outbound sequence after restore.
    /// `#[serde(default)]` keeps markers written by earlier versions
    /// deserializable.
    #[serde(default)]
    pub transport_sequences: Vec<(String, String, u64)>,
}

impl SecurityFingerprint {
    pub fn credential_state(&self, credential_id: &str) -> Option<(u64, u64)> {
        self.credential_states
            .iter()
            .find(|(cid, _, _)| cid == credential_id)
            .map(|(_, g, w)| (*g, *w))
    }

    pub fn sequence_for(&self, issuer_identity_id: &str) -> Option<u64> {
        self.issuer_sequences
            .iter()
            .find(|(iid, _)| iid == issuer_identity_id)
            .map(|(_, s)| *s)
    }

    pub fn transport_sequence_for(&self, issuer: &str, target: &str) -> Option<u64> {
        self.transport_sequences
            .iter()
            .find(|(i, t, _)| i == issuer && t == target)
            .map(|(_, _, s)| *s)
    }
}

/// Pre-restore replay/registry ledger state (SEC-005 BR-05).
///
/// Captured from the live database before the swap and applied as a monotonic
/// overlay after the restore so already-accepted packages stay accepted and
/// producer streams are never silently rewound (SEC-054, F1). `#[serde(default)]`
/// keeps sidecars written by earlier versions deserializable.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RestoreLedgerSnapshot {
    pub applied_packages: Vec<LedgerPackageRow>,
    /// `(issuer_identity_id, last applied sequence)`.
    pub issuer_sequences: Vec<(String, u64)>,
    /// `(issuer_identity_id, target_node_id, last issued sequence)` for every
    /// producer transport stream, applied as a MAX overlay after restore.
    #[serde(default)]
    pub transport_sequences: Vec<(String, String, u64)>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LedgerPackageRow {
    pub package_id: String,
    pub kind: String,
    pub source_node_id: Option<String>,
    pub imported_by: String,
    pub package_sequence: Option<u64>,
    pub issuer_identity_id: Option<String>,
}

/// External restore marker (SEC-005 BR-06). One JSONL line per restore in
/// `<db>.restore.history`. Survives database replacement and drives the
/// boot-time `AuditAction::RestoreBackup` emission.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RestoreMarker {
    pub marker_id: String,
    pub timestamp: String,
    pub backup_path: String,
    /// Operator principal that authorized the restore, when safely available.
    pub session_user_id: Option<String>,
    /// Candidate fingerprint at restore time (what was being restored).
    pub fingerprint: SecurityFingerprint,
    /// Whether the restore was classified as a deliberate security regression.
    pub regressing: bool,
}

/// Commit record for a restore (SEC-005 BR-06). Appended to
/// `<db>.restore.history` ONLY after the atomic swap has committed, so boot can
/// distinguish an attempted restore (marker only) from a committed one.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RestoreMarkerCommit {
    pub marker_id: String,
    pub timestamp: String,
}

/// Path of the append-only restore marker history: `<db>.restore.history`.
pub fn restore_marker_history_path(db_path: &Path) -> PathBuf {
    let mut name = db_path.as_os_str().to_owned();
    name.push(".restore.history");
    PathBuf::from(name)
}

/// Path of the ledger sidecar for one restore: `<db>.restore.ledger.<id>.json`.
pub fn restore_ledger_sidecar_path(db_path: &Path, marker_id: &str) -> PathBuf {
    let mut name = db_path.as_os_str().to_owned();
    name.push(format!(".restore.ledger.{marker_id}.json"));
    PathBuf::from(name)
}

/// Directory where consumed ledger sidecars are archived: `<db>.restore.archive`.
pub fn restore_archive_dir(db_path: &Path) -> PathBuf {
    let mut name = db_path.as_os_str().to_owned();
    name.push(".restore.archive");
    PathBuf::from(name)
}

/// Port for backup operations
pub trait BackupPort: Send + Sync {
    /// Create a backup from a live database connection
    fn create_backup_from_conn(&self, src: &rusqlite::Connection) -> io::Result<PathBuf>;

    /// Create a backup from the database file
    fn create_backup(&self) -> io::Result<PathBuf>;

    /// Check if a backup is needed based on the interval
    fn should_backup(&self) -> bool;

    /// Create a backup if needed
    fn backup_if_needed(&self) -> io::Result<Option<PathBuf>>;

    /// List all backups
    fn get_backup_info(&self) -> io::Result<Vec<BackupInfo>>;

    /// Restore from a backup atomically
    fn restore_backup_atomic(&self, backup_path: &Path) -> io::Result<()>;

    /// Verify integrity of an encrypted backup file.
    /// This will decrypt the file to a temporary location first.
    fn verify_backup_integrity(&self, backup_path: &Path) -> io::Result<String>;

    /// Get the maximum archived year from an encrypted backup file.
    fn get_backup_max_archived_year(&self, backup_path: &Path) -> io::Result<Option<i32>>;

    /// Compute the monotonic security fingerprint of an encrypted backup
    /// candidate and of the live database. Returns `(candidate, live)`.
    fn compute_security_fingerprints(
        &self,
        backup_path: &Path,
    ) -> io::Result<(SecurityFingerprint, SecurityFingerprint)>;

    /// Capture the live replay/registry ledger before a restore swap.
    fn snapshot_restore_ledger(&self) -> io::Result<RestoreLedgerSnapshot>;

    /// Apply a ledger snapshot as a monotonic overlay on the database at
    /// `self.db_path`. Idempotent and transactional: registry is unioned
    /// (INSERT OR IGNORE) and per-issuer sequences advance to the MAX.
    fn apply_ledger_snapshot(&self, snapshot: &RestoreLedgerSnapshot) -> io::Result<()>;
}
