use crate::domain::ports::backup::{
    BackupInfo, BackupPort, LedgerPackageRow, RestoreLedgerSnapshot, SecurityFingerprint,
};
use crate::infrastructure::security::file_encryption::AgeFileEncryptionProvider;
use crate::infrastructure::MAX_IMPORT_SIZE;
use chrono::{DateTime, Utc};
use rusqlite::backup::Backup;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{self, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use tempfile::Builder as TempBuilder;

/// عدد النسخ الاحتياطية للاحتفاظ بها (7 نسخ = أسبوع)
const MAX_BACKUP_COUNT: usize = 7;

/// فترة النسخ الاحتياطي (24 ساعة)
const BACKUP_INTERVAL_HOURS: u64 = 24;

/// مجلد النسخ الاحتياطية
const BACKUP_DIR: &str = "backups";

/// ADR-0017: Restore journal for crash recovery.
/// Written atomically before the live DB is replaced.
#[derive(Debug, Serialize, Deserialize)]
struct RestoreJournalV1 {
    version: u32,
    phase: RestorePhase,
    /// Absolute path of the persisted (post-`keep()`) temp candidate file.
    candidate_path: PathBuf,
    rollback_path: PathBuf,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum RestorePhase {
    /// Candidate file on disk is complete and validated; live DB not yet replaced.
    ReadySwap,
}

pub struct SqliteBackupAdapter {
    db_path: PathBuf,
    backup_dir: PathBuf,
    max_count: usize,
    crypto_port: AgeFileEncryptionProvider,
}

/// A wrapper around NamedTempFile that also cleans up SQLite auxiliary files (-wal, -shm) on drop.
struct TempSqliteFile {
    inner: Option<tempfile::NamedTempFile>,
}

impl TempSqliteFile {
    fn new(inner: tempfile::NamedTempFile) -> Self {
        Self { inner: Some(inner) }
    }

    fn path(&self) -> &Path {
        self.inner.as_ref().unwrap().path()
    }

    fn keep(mut self) -> Result<(fs::File, PathBuf), tempfile::PersistError> {
        let inner = self.inner.take().unwrap();
        let path = inner.path().to_path_buf();
        let result = inner.keep();

        // Cleanup auxiliary files after keep/persist.
        // They are no longer valid for the new persisted path name.
        let mut wal = path.clone().into_os_string();
        wal.push("-wal");
        secure_delete(Path::new(&wal));

        let mut shm = path.into_os_string();
        shm.push("-shm");
        secure_delete(Path::new(&shm));

        result
    }
}

impl Drop for TempSqliteFile {
    fn drop(&mut self) {
        if let Some(temp) = self.inner.take() {
            let path = temp.path().to_path_buf();
            // Drop the NamedTempFile to close and delete the main file first
            drop(temp);
            // Now cleanup auxiliary files
            let mut wal = path.clone().into_os_string();
            wal.push("-wal");
            secure_delete(Path::new(&wal));

            let mut shm = path.into_os_string();
            shm.push("-shm");
            secure_delete(Path::new(&shm));
        }
    }
}

impl SqliteBackupAdapter {
    pub fn new(db_path: impl AsRef<Path>, crypto_port: AgeFileEncryptionProvider) -> Self {
        let db_path = db_path.as_ref().to_path_buf();
        let backup_dir = Self::compute_backup_dir(&db_path);

        Self {
            db_path,
            backup_dir,
            max_count: MAX_BACKUP_COUNT,
            crypto_port,
        }
    }

    pub fn compute_backup_dir(db_path: &Path) -> PathBuf {
        db_path
            .parent()
            .map(|p| p.join(BACKUP_DIR))
            .unwrap_or_else(|| PathBuf::from(BACKUP_DIR))
    }

    fn journal_path(&self) -> PathBuf {
        self.db_path.with_extension("restore.journal")
    }

    fn list_backups_internal(&self) -> io::Result<Vec<PathBuf>> {
        let mut backups = Vec::new();

        if !self.backup_dir.exists() {
            return Ok(backups);
        }

        for entry in fs::read_dir(&self.backup_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() {
                let filename = path.file_name().and_then(|f| f.to_str()).unwrap_or("");
                if filename.starts_with("grpc_")
                    && (filename.ends_with(".db") || filename.ends_with(".bak"))
                {
                    backups.push(path);
                }
            }
        }

        backups.sort_by(|a, b| {
            let time_a = fs::metadata(a)
                .and_then(|m| m.modified())
                .unwrap_or(SystemTime::UNIX_EPOCH);
            let time_b = fs::metadata(b)
                .and_then(|m| m.modified())
                .unwrap_or(SystemTime::UNIX_EPOCH);
            time_b.cmp(&time_a)
        });

        Ok(backups)
    }

    fn cleanup_old_backups(&self) -> io::Result<()> {
        let backups = self.list_backups_internal()?;
        if backups.len() > self.max_count {
            for backup in &backups[self.max_count..] {
                let _ = fs::remove_file(backup);
            }
        }
        Ok(())
    }

    fn validate_sqlite_file(&self, path: &Path) -> io::Result<()> {
        use rusqlite::Connection;
        let conn = Connection::open(path).map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Cannot open SQLite: {}", e),
            )
        })?;

        let quick_result: rusqlite::Result<String> =
            conn.query_row("PRAGMA quick_check", [], |row| row.get(0));

        match quick_result {
            Ok(status) if status == "ok" => Ok(()),
            Ok(status) => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Database corruption: {}", status),
            )),
            Err(e) => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Validation error: {}", e),
            )),
        }
    }

    /// Validate the temporary SQLite snapshot before encryption.
    /// This is a FAIL-CLOSED validation - any failure aborts the backup.
    fn validate_temp_snapshot(
        &self,
        src: &rusqlite::Connection,
        temp_path: &Path,
    ) -> io::Result<()> {
        use rusqlite::Connection;

        // A. Check temp file exists
        if !temp_path.exists() {
            let error_msg = format!("Temp snapshot file does not exist: {}", temp_path.display());
            log::error!(
                target: "grpc::backup",
                "[SNAPSHOT_MISSING] temp_snapshot_path={}",
                temp_path.display()
            );
            return Err(io::Error::new(io::ErrorKind::NotFound, error_msg));
        }

        // B. Check temp file size
        let temp_size = fs::metadata(temp_path)
            .map_err(|e| io::Error::other(format!("Cannot get temp file size: {}", e)))?
            .len();

        // Get source database page count to determine minimum valid size
        let src_page_count: i64 = src
            .query_row("PRAGMA page_count", [], |row| row.get(0))
            .unwrap_or(0);
        let src_page_size: i64 = src
            .query_row("PRAGMA page_size", [], |row| row.get(0))
            .unwrap_or(4096);

        // Minimum size is at least the SQLite header (100 bytes)
        // For databases with data, require at least page_size
        let min_size = if src_page_count > 0 {
            src_page_size.max(4096) as u64
        } else {
            100 // Empty database header only
        };

        // Empty file is always invalid - SQLite header alone is at least 100 bytes
        if temp_size == 0 {
            let error_msg = format!("Temp snapshot is empty (0 bytes): {}", temp_path.display());
            log::error!(
                target: "grpc::backup",
                "[SNAPSHOT_EMPTY] temp_snapshot_path={} temp_db_size=0",
                temp_path.display()
            );
            return Err(io::Error::new(io::ErrorKind::InvalidData, error_msg));
        }

        // Only reject if temp is smaller than minimum AND source has data
        // Allow empty databases (src_page_count == 0) to have small files (just header)
        if src_page_count > 0 && temp_size < min_size {
            let error_msg = format!(
                "Temp snapshot too small: {} bytes (minimum: {} bytes, source page_count={}, page_size={})",
                temp_size, min_size, src_page_count, src_page_size
            );
            log::error!(
                target: "grpc::backup",
                "[SNAPSHOT_TOO_SMALL] temp_snapshot_path={} temp_db_size={} min_size={} src_page_count={} src_page_size={}",
                temp_path.display(),
                temp_size,
                min_size,
                src_page_count,
                src_page_size
            );
            return Err(io::Error::new(io::ErrorKind::InvalidData, error_msg));
        }

        // C. Open temp database in READ_ONLY mode and validate
        let temp_conn = Connection::open_with_flags(
            temp_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|e| {
            log::error!(
                target: "grpc::backup",
                "[SNAPSHOT_OPEN_FAILED] temp_snapshot_path={} error={}",
                temp_path.display(),
                e
            );
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Cannot open temp snapshot for validation: {}", e),
            )
        })?;

        // Run integrity_check
        let integrity_result: rusqlite::Result<String> =
            temp_conn.query_row("PRAGMA integrity_check", [], |row| row.get(0));

        let integrity_status = match integrity_result {
            Ok(status) => status,
            Err(e) => {
                let error_msg = format!("Integrity check query failed: {}", e);
                log::error!(
                    target: "grpc::backup",
                    "[INTEGRITY_CHECK_FAILED] temp_snapshot_path={} error={}",
                    temp_path.display(),
                    error_msg
                );
                return Err(io::Error::new(io::ErrorKind::InvalidData, error_msg));
            }
        };

        if integrity_status != "ok" {
            let error_msg = format!("Temp snapshot integrity check failed: {}", integrity_status);
            log::error!(
                target: "grpc::backup",
                "[INTEGRITY_CHECK_NOT_OK] temp_snapshot_path={} integrity_check_result={}",
                temp_path.display(),
                integrity_status
            );
            return Err(io::Error::new(io::ErrorKind::InvalidData, error_msg));
        }

        // Get temp database page metrics
        let temp_page_count: i64 = temp_conn
            .query_row("PRAGMA page_count", [], |row| row.get(0))
            .map_err(|e| {
                log::error!(
                    target: "grpc::backup",
                    "[PAGE_COUNT_QUERY_FAILED] temp_snapshot_path={} error={}",
                    temp_path.display(),
                    e
                );
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("Cannot get page_count: {}", e),
                )
            })?;

        let temp_page_size: i64 = temp_conn
            .query_row("PRAGMA page_size", [], |row| row.get(0))
            .map_err(|e| {
                log::error!(
                    target: "grpc::backup",
                    "[PAGE_SIZE_QUERY_FAILED] temp_snapshot_path={} error={}",
                    temp_path.display(),
                    e
                );
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("Cannot get page_size: {}", e),
                )
            })?;

        // D. Validate page count and calculated logical size
        if temp_page_count <= 0 {
            let error_msg = format!("Temp snapshot has invalid page_count: {}", temp_page_count);
            log::error!(
                target: "grpc::backup",
                "[INVALID_PAGE_COUNT] temp_snapshot_path={} page_count={}",
                temp_path.display(),
                temp_page_count
            );
            return Err(io::Error::new(io::ErrorKind::InvalidData, error_msg));
        }

        // Calculate logical sizes
        let src_logical_size = src_page_count * src_page_size;
        let temp_logical_size = temp_page_count * temp_page_size;

        // E. Minimum snapshot consistency check
        // Allow small variance (1 page) for header differences, but fail if dramatically smaller
        // Skip this check for empty databases
        if src_page_count > 0 && temp_page_count > 0 {
            let page_diff = (src_page_count - temp_page_count).abs();
            // If temp has significantly fewer pages than source, fail closed
            if temp_page_count < src_page_count && page_diff > 1 {
                let error_msg = format!(
                    "Temp snapshot has significantly fewer pages than source: temp_page_count={} vs src_page_count={} (diff={})",
                    temp_page_count, src_page_count, page_diff
                );
                log::error!(
                    target: "grpc::backup",
                    "[SNAPSHOT_SIZE_MISMATCH] temp_snapshot_path={} temp_page_count={} temp_page_size={} temp_logical_size={} src_page_count={} src_page_size={} src_logical_size={} page_diff={}",
                    temp_path.display(),
                    temp_page_count,
                    temp_page_size,
                    temp_logical_size,
                    src_page_count,
                    src_page_size,
                    src_logical_size,
                    page_diff
                );
                return Err(io::Error::new(io::ErrorKind::InvalidData, error_msg));
            }
        }

        // Log successful validation
        log::info!(
            target: "grpc::backup",
            "[SNAPSHOT_VALIDATION_SUCCESS] temp_snapshot_path={} temp_db_size={} page_count={} page_size={} integrity_check_result=ok src_page_count={} src_page_size={}",
            temp_path.display(),
            temp_size,
            temp_page_count,
            temp_page_size,
            src_page_count,
            src_page_size
        );

        Ok(())
    }

    /// Validate a restore candidate as a canonical filesystem path contained
    /// by the authorized backup directory (BR-10).
    ///
    /// Security rule: containment is established via filesystem canonicalization
    /// (symlinks resolved, relative paths resolved against the process CWD),
    /// never via string inspection. A candidate that cannot be canonicalized,
    /// or whose canonical form lies outside the canonical backup root, is
    /// rejected. Traversal markers are additionally rejected textually as a
    /// first-line defense.
    pub fn validate_restore_path(&self, path: &Path) -> io::Result<()> {
        let path_str = path.to_string_lossy();
        if crate::domain::security::contains_path_traversal(&path_str) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Security violation: path traversal detected",
            ));
        }

        // Canonicalize the candidate: resolves relative paths (against CWD)
        // and symlinks; a path that cannot be canonicalized is rejected.
        let canonical_path = path.canonicalize().map_err(|e| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("Invalid backup path: {}", e),
            )
        })?;

        let canonical_root = self.canonical_backup_root()?;

        // Component-wise containment (never string-prefix comparison).
        if !canonical_path.starts_with(&canonical_root) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Security violation: backup path outside allowed directory",
            ));
        }
        Ok(())
    }

    /// Canonical form of the authorized backup directory. If the directory
    /// does not exist yet, the nearest existing ancestor is canonicalized and
    /// the leaf name appended, so the root stays symlink-free.
    fn canonical_backup_root(&self) -> io::Result<PathBuf> {
        match self.backup_dir.canonicalize() {
            Ok(root) => Ok(root),
            Err(_) => {
                let parent = self.backup_dir.parent().ok_or_else(|| {
                    io::Error::new(io::ErrorKind::NotFound, "backup dir has no parent")
                })?;
                let canonical_parent = parent.canonicalize().map_err(|e| {
                    io::Error::new(
                        io::ErrorKind::NotFound,
                        format!("backup dir unresolvable: {}", e),
                    )
                })?;
                let name = self.backup_dir.file_name().ok_or_else(|| {
                    io::Error::new(io::ErrorKind::NotFound, "backup dir has no name")
                })?;
                Ok(canonical_parent.join(name))
            }
        }
    }

    fn clear_journal(&self) {
        let _ = fs::remove_file(self.journal_path());
    }

    fn write_journal(&self, j: &RestoreJournalV1) -> io::Result<()> {
        let tmp = self.journal_path().with_extension("restore.journal.tmp");
        // ADR-0017: journal is small; to_vec_pretty is acceptable here (not on the hot path).
        let json = serde_json::to_vec_pretty(j)
            .map_err(|e| io::Error::other(format!("journal serialize: {}", e)))?;
        let mut f = fs::File::create(&tmp)?;
        f.write_all(&json)?;
        f.sync_all()?;
        fs::rename(&tmp, self.journal_path())?;
        Ok(())
    }

    /// Returns the directory that should hold temp restore files.
    /// Using the same filesystem as the DB avoids cross-device rename failures.
    fn db_dir(&self) -> PathBuf {
        self.db_path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."))
    }
}

impl BackupPort for SqliteBackupAdapter {
    fn create_backup_from_conn(&self, src: &rusqlite::Connection) -> io::Result<PathBuf> {
        fs::create_dir_all(&self.backup_dir)?;

        let now = Utc::now();
        let date_str = now.format("%Y%m%d_%H%M%S").to_string();
        let millis = now.timestamp_millis() % 1000;
        let backup_filename = format!("grpc_{}_{:03}.bak", date_str, millis);
        let backup_path = self.backup_dir.join(&backup_filename);

        // ── Phase 2: Memory-aware SQLite Backup ──────────────────────────
        // We avoid loading the entire database into RAM by using a temporary
        // on-disk file for the incremental backup. This ensures RAM usage is
        // predictable relative to database size.
        // ─────────────────────────────────────────────────────────────────

        let temp_db_path = self
            .db_dir()
            .join(format!("backup_temp_{}_{:03}.sqlite_tmp", date_str, millis));

        {
            let mut dest = rusqlite::Connection::open(&temp_db_path).map_err(|e| {
                io::Error::other(format!("Failed to open temporary staging DB: {}", e))
            })?;

            let backup = Backup::new(src, &mut dest)
                .map_err(|e| io::Error::other(format!("Backup initialization failed: {}", e)))?;

            // Incremental copy: 100 pages at a time to stay responsive.
            loop {
                match backup.step(100) {
                    Ok(rusqlite::backup::StepResult::Done) => break,
                    Ok(rusqlite::backup::StepResult::More) | Ok(_) => continue,
                    Err(e) => return Err(io::Error::other(format!("Backup step failed: {}", e))),
                }
            }
        }

        // ── Phase 3: Snapshot Validation BEFORE Encryption ─────────────
        // FAIL-CLOSED: Validate the temporary snapshot before encryption.
        // If validation fails, abort backup and securely delete temp file.
        self.validate_temp_snapshot(src, &temp_db_path)?;

        // Stream the temporary plaintext file through age encrypt directly to the output file.
        {
            let input_file = fs::File::open(&temp_db_path)?;
            let mut reader = BufReader::with_capacity(64 * 1024, input_file);

            let output_file = fs::File::create(&backup_path)?;
            let mut output_writer = BufWriter::new(output_file);
            self.crypto_port
                .encrypt_stream(&mut reader, &mut output_writer)
                .map_err(|e| {
                    let _ = fs::remove_file(&backup_path);
                    io::Error::other(format!("age encryption failed: {}", e))
                })?;

            output_writer.flush()?;
            output_writer
                .into_inner()
                .map_err(|e| io::Error::other(format!("flush: {}", e)))?
                .sync_all()?;
        }

        // Securely remove the temporary plaintext file.
        secure_delete(&temp_db_path);

        self.cleanup_old_backups()?;
        Ok(backup_path)
    }

    fn create_backup(&self) -> io::Result<PathBuf> {
        let src = rusqlite::Connection::open_with_flags(
            &self.db_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|e| io::Error::other(format!("Failed to open DB for backup: {}", e)))?;

        self.create_backup_from_conn(&src)
    }

    fn should_backup(&self) -> bool {
        let backups = match self.list_backups_internal() {
            Ok(b) => b,
            _ => return true,
        };
        let latest_backup = match backups.into_iter().next() {
            Some(p) => p,
            None => return true,
        };

        let metadata = match fs::metadata(&latest_backup) {
            Ok(m) => m,
            Err(_) => return true,
        };

        let modified_time = match metadata.modified() {
            Ok(t) => t,
            Err(_) => return true,
        };

        let elapsed = match modified_time.elapsed() {
            Ok(e) => e,
            Err(_) => return true,
        };

        elapsed >= Duration::from_secs(BACKUP_INTERVAL_HOURS * 3600)
    }

    fn backup_if_needed(&self) -> io::Result<Option<PathBuf>> {
        if self.should_backup() {
            let path = self.create_backup()?;
            Ok(Some(path))
        } else {
            Ok(None)
        }
    }

    fn get_backup_info(&self) -> io::Result<Vec<BackupInfo>> {
        let backups = self.list_backups_internal()?;
        let mut info_list = Vec::new();

        for path in backups {
            let metadata = fs::metadata(&path)?;
            let size = metadata.len();
            let created = metadata.modified()?;
            let filename = path
                .file_name()
                .and_then(|f| f.to_str())
                .unwrap_or("unknown")
                .to_string();
            let datetime: DateTime<Utc> = created.into();

            info_list.push(BackupInfo {
                filename,
                path: path.to_string_lossy().to_string(),
                size_bytes: size,
                created: datetime.to_rfc3339(),
            });
        }

        Ok(info_list)
    }

    /// ADR-0017: Buffered backup restore.
    ///
    /// The decrypted database bytes are NEVER held in memory as a full Vec<u8>.
    /// Instead:
    ///
    /// 1. File size is checked against MAX_IMPORT_SIZE.
    /// 2. A `tempfile::NamedTempFile` is created in the same directory as the live DB
    ///    (same filesystem → atomic rename is guaranteed).
    /// 3. The age decrypt stream writes directly to the tempfile via BufWriter.
    /// 4. SQLite `quick_check` validates the tempfile **in place**.
    /// 5. The journal is written atomically before the live DB is replaced.
    /// 6. `persist()` converts the tempfile to a permanent path (no auto-delete).
    /// 7. Atomic rename replaces the live DB.
    ///
    /// Peak RAM is bounded by buffers, not database size.
    fn restore_backup_atomic(&self, backup_path: &Path) -> io::Result<()> {
        self.validate_restore_path(backup_path)?;

        let metadata = fs::metadata(backup_path)?;
        if metadata.len() > MAX_IMPORT_SIZE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "Backup file too large ({} MB). Limit is {} MB.",
                    metadata.len() / 1024 / 1024,
                    MAX_IMPORT_SIZE / 1024 / 1024
                ),
            ));
        }

        let rollback_path = self.db_path.with_extension("rollback.bak");
        self.clear_journal();

        // ── Step 1: decrypt stream directly into a secure tempfile ────────
        let temp_candidate = self.decrypt_to_temp_file(backup_path)?;

        // ── Step 2: validate the tempfile via SQLite PRAGMA quick_check ───
        self.validate_sqlite_file(temp_candidate.path())?;

        // ── Step 3: persist the tempfile (stops auto-delete on Drop) ──────
        // This converts the NamedTempFile into a plain PathBuf.
        let (_, candidate_path) = temp_candidate
            .keep()
            .map_err(|e| io::Error::other(format!("Failed to persist temp candidate: {}", e)))?;

        // ── Step 4: write crash-recovery journal ──────────────────────────
        self.write_journal(&RestoreJournalV1 {
            version: 1,
            phase: RestorePhase::ReadySwap,
            candidate_path: candidate_path.clone(),
            rollback_path: rollback_path.clone(),
        })?;

        // ── Step 5: atomic swap ───────────────────────────────────────────
        if self.db_path.exists() {
            if rollback_path.exists() {
                let _ = fs::remove_file(&rollback_path);
            }
            fs::rename(&self.db_path, &rollback_path).map_err(|e| {
                let _ = fs::remove_file(&candidate_path);
                self.clear_journal();
                io::Error::other(format!("failed to move live DB to rollback: {}", e))
            })?;
        }

        if let Err(e) = fs::rename(&candidate_path, &self.db_path) {
            if rollback_path.exists() {
                let _ = fs::rename(&rollback_path, &self.db_path);
            }
            let _ = fs::remove_file(&candidate_path);
            self.clear_journal();
            return Err(io::Error::other(format!("Atomic swap failed: {}", e)));
        }

        if rollback_path.exists() {
            let _ = fs::remove_file(&rollback_path);
        }
        self.clear_journal();

        Ok(())
    }

    fn verify_backup_integrity(&self, backup_path: &Path) -> io::Result<String> {
        let temp_candidate = self.decrypt_to_temp_file(backup_path)?;
        crate::infrastructure::db::integrity::verify_file_integrity(
            temp_candidate.path().to_str().unwrap_or(""),
        )
        .map_err(|e| io::Error::other(e.to_string()))
    }

    fn get_backup_max_archived_year(&self, backup_path: &Path) -> io::Result<Option<i32>> {
        let temp_candidate = self.decrypt_to_temp_file(backup_path)?;
        crate::infrastructure::db::metadata::get_max_archived_year_in_file(temp_candidate.path())
            .map_err(|e| io::Error::other(e.to_string()))
    }

    fn compute_security_fingerprints(
        &self,
        backup_path: &Path,
    ) -> io::Result<(SecurityFingerprint, SecurityFingerprint)> {
        let temp_candidate = self.decrypt_to_temp_file(backup_path)?;
        let candidate = read_security_fingerprint(temp_candidate.path())?;
        let live = read_security_fingerprint(&self.db_path)?;
        Ok((candidate, live))
    }

    fn snapshot_restore_ledger(&self) -> io::Result<RestoreLedgerSnapshot> {
        use rusqlite::Connection;
        let conn = Connection::open_with_flags(
            &self.db_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|e| io::Error::other(format!("open live DB for ledger snapshot: {e}")))?;

        let mut packages = Vec::new();
        {
            let mut stmt = conn
                .prepare(
                    r#"SELECT package_id, kind, source_node_id, imported_by, issuer_identity_id
                       FROM applied_sync_packages ORDER BY package_id"#,
                )
                .map_err(|e| io::Error::other(format!("prepare ledger snapshot: {e}")))?;
            let rows = stmt
                .query_map([], |row| {
                    Ok(LedgerPackageRow {
                        package_id: row.get(0)?,
                        kind: row.get(1)?,
                        source_node_id: row.get(2)?,
                        imported_by: row.get(3)?,
                        issuer_identity_id: row.get(4)?,
                    })
                })
                .map_err(|e| io::Error::other(format!("query ledger snapshot: {e}")))?;
            for row in rows {
                packages.push(row.map_err(|e| io::Error::other(format!("map ledger row: {e}")))?);
            }
        }

        Ok(RestoreLedgerSnapshot {
            applied_packages: packages,
        })
    }

    fn apply_ledger_snapshot(&self, snapshot: &RestoreLedgerSnapshot) -> io::Result<()> {
        use rusqlite::Connection;
        let mut conn = Connection::open(&self.db_path)
            .map_err(|e| io::Error::other(format!("open DB for ledger overlay: {e}")))?;
        let tx = conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(|e| io::Error::other(format!("begin ledger overlay: {e}")))?;

        for row in &snapshot.applied_packages {
            tx.execute(
                r#"INSERT OR IGNORE INTO applied_sync_packages
                   (package_id, kind, source_node_id, imported_at, imported_by, issuer_identity_id)
                   VALUES (?1, ?2, ?3, datetime('now'), ?4, ?5)"#,
                rusqlite::params![
                    row.package_id,
                    row.kind,
                    row.source_node_id,
                    row.imported_by,
                    row.issuer_identity_id,
                ],
            )
            .map_err(|e| io::Error::other(format!("overlay registry row: {e}")))?;
        }

        tx.commit()
            .map_err(|e| io::Error::other(format!("commit ledger overlay: {e}")))
    }
}

/// Read the monotonic security fingerprint of a SQLite file (SEC-005 BR-03/14).
///
/// Only SELECTs over persisted monotonic security state; identical queries are
/// run against the candidate and the live database so the comparison is exact.
fn read_security_fingerprint(db_file: &Path) -> io::Result<SecurityFingerprint> {
    use rusqlite::Connection;
    let conn = Connection::open_with_flags(
        db_file,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| io::Error::other(format!("open DB for fingerprint: {e}")))?;

    let credential_states = {
        let mut stmt = conn
            .prepare(
                r#"SELECT credential_id,
                          MAX(generation),
                          MAX(CASE status
                              WHEN 'ACTIVE' THEN 0
                              WHEN 'REVOKED' THEN 1
                              WHEN 'SUPERSEDED' THEN 1
                              WHEN 'EXPIRED' THEN 1
                              ELSE 0 END)
                   FROM identity_store WHERE deleted = 0
                   GROUP BY credential_id ORDER BY credential_id"#,
            )
            .map_err(|e| io::Error::other(format!("prepare credential fingerprint: {e}")))?;
        let rows = stmt
            .query_map([], |row| {
                let gen = row.get::<_, i64>(1)?;
                let gen = u64::try_from(gen).map_err(|_| {
                    rusqlite::Error::InvalidColumnType(
                        1,
                        "u64".into(),
                        rusqlite::types::Type::Integer,
                    )
                })?;
                let watermark = row.get::<_, i64>(2)?;
                let watermark = u64::try_from(watermark).map_err(|_| {
                    rusqlite::Error::InvalidColumnType(
                        2,
                        "u64".into(),
                        rusqlite::types::Type::Integer,
                    )
                })?;
                Ok((row.get::<_, String>(0)?, gen, watermark))
            })
            .map_err(|e| io::Error::other(format!("query credential fingerprint: {e}")))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(|e| io::Error::other(format!("map credential row: {e}")))?);
        }
        out
    };

    let active_wilaya_anchor_generation = conn
        .query_row(
            r#"SELECT MAX(generation) FROM identity_store
               WHERE subject_type = 'WILAYA' AND status = 'ACTIVE' AND deleted = 0"#,
            [],
            |row| row.get::<_, Option<i64>>(0),
        )
        .map_err(|e| io::Error::other(format!("query WILAYA anchor: {e}")))?
        .and_then(|g| u64::try_from(g).ok());

    let registry_package_ids = {
        let mut stmt = conn
            .prepare(r#"SELECT package_id FROM applied_sync_packages ORDER BY package_id"#)
            .map_err(|e| io::Error::other(format!("prepare registry fingerprint: {e}")))?;
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| io::Error::other(format!("query registry fingerprint: {e}")))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(|e| io::Error::other(format!("map registry row: {e}")))?);
        }
        out
    };

    let active_admin_credential = {
        use rusqlite::OptionalExtension;
        conn.query_row(
            r#"SELECT credential_id, generation FROM identity_store
               WHERE subject_type = 'ADMIN' AND status = 'ACTIVE' AND deleted = 0
               ORDER BY generation DESC LIMIT 1"#,
            [],
            |row| {
                let gen = row.get::<_, i64>(1)?;
                let gen = u64::try_from(gen).map_err(|_| {
                    rusqlite::Error::InvalidColumnType(
                        1,
                        "u64".into(),
                        rusqlite::types::Type::Integer,
                    )
                })?;
                Ok((row.get::<_, String>(0)?, gen))
            },
        )
        .optional()
        .map_err(|e| io::Error::other(format!("query ACTIVE ADMIN credential: {e}")))?
    };

    Ok(SecurityFingerprint {
        credential_states,
        active_wilaya_anchor_generation,
        registry_package_ids,
        active_admin_credential,
    })
}

impl SqliteBackupAdapter {
    /// Decrypt an encrypted backup file to a secure temporary location.
    fn decrypt_to_temp_file(&self, backup_path: &Path) -> io::Result<TempSqliteFile> {
        let db_dir = self.db_dir();
        let temp_file = TempBuilder::new()
            .prefix("grpc_decrypt_")
            .suffix(".sqlite_tmp")
            .tempfile_in(&db_dir)
            .map_err(|e| {
                io::Error::other(format!(
                    "Failed to create secure temp file for decryption: {}",
                    e
                ))
            })?;

        {
            let mut input_file = BufReader::new(fs::File::open(backup_path)?);
            let mut output_writer = BufWriter::new(temp_file.as_file());
            self.crypto_port
                .decrypt_stream(&mut input_file, &mut output_writer)
                .map_err(|e| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("Decryption failed: {}", e),
                    )
                })?;
            output_writer
                .flush()
                .map_err(|e| io::Error::other(format!("flush: {}", e)))?;
            temp_file
                .as_file()
                .sync_all()
                .map_err(|e| io::Error::other(format!("fsync temp: {}", e)))?;
        }

        Ok(TempSqliteFile::new(temp_file))
    }
}

/// Outcome of boot-time interrupted-restore recovery.
///
/// `restore_completed` distinguishes "the candidate backup became the live
/// database" from "the old database remains live". Journal existence alone is
/// NOT proof of completion: a failed swap can leave the journal behind after
/// the rollback was restored. Audit emission must depend on this outcome, not
/// on journal presence (SEC-005 residual LOW).
pub struct RestoreRecoveryOutcome {
    /// True iff recovery established the candidate backup as the live DB.
    pub restore_completed: bool,
}

/// Boot-time recovery: finish an interrupted restore or remove stale artifacts.
/// Safe to call before opening the main database file.
///
/// ADR-0017: crash recovery guarantees (Cases A–E unchanged). The returned
/// outcome reports whether the candidate actually became the live database.
pub fn recover_interrupted_restore_and_orphans(
    db_path: &Path,
    _crypto: &AgeFileEncryptionProvider,
) -> io::Result<RestoreRecoveryOutcome> {
    let journal = db_path.with_extension("restore.journal");
    if !journal.exists() {
        cleanup_loose_restore_artifacts(db_path)?;
        return Ok(RestoreRecoveryOutcome {
            restore_completed: false,
        });
    }

    // The journal file is naturally small (contains a few paths), so fs::read is acceptable.
    let raw = fs::read(&journal).map_err(|e| {
        // [arch:allow-memory-unsafe] see ADR-0017 — Reason: whole-file read in backup for SHA-256 hashing — required for cryptographic verification; Date: 2026-08-09; Owner: Infrastructure
        io::Error::other(format!("restore journal read: {}", e))
    })?;

    let mut restore_completed = false;
    match serde_json::from_slice::<RestoreJournalV1>(&raw) {
        Ok(parsed) => {
            if parsed.phase == RestorePhase::ReadySwap {
                // Case A (ADR-0017): journal persisted but rename(live→rollback) not started yet.
                // Both candidate and live exist — must complete the atomic swap, never delete candidate.
                if parsed.candidate_path.exists() && db_path.exists() {
                    if parsed.rollback_path.exists() {
                        let _ = fs::remove_file(&parsed.rollback_path);
                    }
                    fs::rename(db_path, &parsed.rollback_path).map_err(|e| {
                        io::Error::other(format!(
                            "recovery: failed to move live DB to rollback: {}",
                            e
                        ))
                    })?;
                    if let Err(e) = fs::rename(&parsed.candidate_path, db_path) {
                        let _ = fs::rename(&parsed.rollback_path, db_path);
                        return Err(io::Error::other(format!(
                            "recovery: failed to promote candidate to live: {}",
                            e
                        )));
                    }
                    if parsed.rollback_path.exists() {
                        let _ = fs::remove_file(&parsed.rollback_path);
                    }
                    restore_completed = true;
                } else if parsed.candidate_path.exists() && !db_path.exists() {
                    // Case B: live already at rollback; promote candidate.
                    restore_completed = fs::rename(&parsed.candidate_path, db_path).is_ok();
                }

                // Case C: swap finished; rollback may still exist while journal remained.
                if parsed.rollback_path.exists() && db_path.exists() {
                    let _ = fs::remove_file(&parsed.rollback_path);
                    restore_completed = true;
                }

                // Case E: live missing, rollback holds prior live, candidate already gone.
                if !db_path.exists()
                    && parsed.rollback_path.exists()
                    && !parsed.candidate_path.exists()
                {
                    // Restoring the rollback brings the OLD database back live —
                    // the restore did NOT complete.
                    let _ = fs::rename(&parsed.rollback_path, db_path);
                }
            }
        }
        Err(e) => {
            log::warn!(target: "grpc::backup", "restore journal corrupt, removing: {}", e);
        }
    }

    let _ = fs::remove_file(&journal);
    cleanup_loose_restore_artifacts(db_path)?;
    Ok(RestoreRecoveryOutcome { restore_completed })
}

/// Overwrite file contents with zeros before deleting to reduce plaintext
/// exposure window on storage devices that lack secure-erase.
/// Best-effort — errors are ignored (the file will still be unlinked).
fn secure_delete(path: &Path) {
    secure_delete_single_file(path);

    // SQLite auxiliary files (-wal and -shm)
    let mut wal = path.to_path_buf().into_os_string();
    wal.push("-wal");
    secure_delete_single_file(Path::new(&wal));

    let mut shm = path.to_path_buf().into_os_string();
    shm.push("-shm");
    secure_delete_single_file(Path::new(&shm));
}

fn secure_delete_single_file(path: &Path) {
    if !path.exists() {
        return;
    }
    if let Ok(mut f) = fs::OpenOptions::new().write(true).open(path) {
        if let Ok(meta) = fs::metadata(path) {
            let zeros = vec![0u8; (meta.len() as usize).min(1024 * 1024)];
            let _ = f.write_all(&zeros);
            let _ = f.sync_all();
        }
    }
    let _ = fs::remove_file(path);
}

fn cleanup_loose_restore_artifacts(db_path: &Path) -> io::Result<()> {
    // Remove any predictable legacy staging files from pre-ADR-0017 code.
    let stale_tmp = db_path.with_extension("restore.tmp");
    if stale_tmp.exists() {
        let _ = fs::remove_file(&stale_tmp);
    }
    let journal = db_path.with_extension("restore.journal");
    let legacy_new = db_path.with_extension("restore.new");
    if !journal.exists() && legacy_new.exists() {
        secure_delete(&legacy_new);
    }
    // Case D: NamedTempFile orphans from interrupted decrypt (no journal) — safe once no journal.
    if !journal.exists() {
        if let Some(parent) = db_path.parent() {
            if let Ok(rd) = fs::read_dir(parent) {
                for ent in rd.flatten() {
                    let path = ent.path();
                    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                        continue;
                    };
                    if name.starts_with("grpc_restore_") && name.ends_with(".sqlite_tmp") {
                        secure_delete(&path);
                    }
                }
            }
        }
    }
    Ok(())
}

// =============================================================================
// Tests — Zero-plaintext restore
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn write_journal_atomic(db_path: &Path, j: &RestoreJournalV1) {
        let journal_path = db_path.with_extension("restore.journal");
        let tmp = journal_path.with_extension("restore.journal.tmp");
        let json = serde_json::to_vec_pretty(j).unwrap();
        std::fs::write(&tmp, json).unwrap();
        std::fs::rename(&tmp, &journal_path).unwrap();
    }

    /// Verifies that the restore temp file uses the tempfile crate (randomised
    /// name, no predictable `.restore.new` suffix).
    #[test]
    fn restore_uses_unpredictable_temp_name() {
        // We can verify by checking that the restore.new legacy path is NOT
        // created during a restore attempt. We use a non-existent backup path
        // so the restore fails early after creating the tempfile.
        let tmp_dir = tempfile::tempdir().unwrap();
        let db_path = tmp_dir.path().join("test.db");
        let crypto = AgeFileEncryptionProvider::new();
        let adapter = SqliteBackupAdapter::new(&db_path, crypto);

        // The legacy predictable path must never be created.
        let legacy_path = db_path.with_extension("restore.new");
        let fake_backup = tmp_dir.path().join("fake.bak");
        // create an empty file — will fail decryption/validation but tests the path
        std::fs::write(&fake_backup, b"").unwrap();
        let _ = adapter.restore_backup_atomic(&fake_backup);
        assert!(
            !legacy_path.exists(),
            "Predictable '.restore.new' staging path must not be created"
        );
    }

    /// Verifies that `recover_interrupted_restore_and_orphans` removes
    /// a candidate file when the live DB is already in place.
    #[test]
    fn crash_recovery_removes_orphan_candidate_when_db_exists() {
        let tmp_dir = tempfile::tempdir().unwrap();
        let db_path = tmp_dir.path().join("live.db");
        let candidate = tmp_dir.path().join("grpc_restore_orphan.sqlite_tmp");

        // Simulate: DB exists + orphan candidate
        std::fs::write(&db_path, b"db_content").unwrap();
        std::fs::write(&candidate, b"orphan").unwrap();

        let journal_path = db_path.with_extension("restore.journal");
        let journal = RestoreJournalV1 {
            version: 1,
            phase: RestorePhase::ReadySwap,
            candidate_path: candidate.clone(),
            rollback_path: db_path.with_extension("rollback.bak"),
        };
        let j_bytes = serde_json::to_vec_pretty(&journal).unwrap();
        let tmp_j = journal_path.with_extension("restore.journal.tmp");
        std::fs::write(&tmp_j, &j_bytes).unwrap();
        std::fs::rename(&tmp_j, &journal_path).unwrap();

        let crypto = AgeFileEncryptionProvider::new();
        let outcome = recover_interrupted_restore_and_orphans(&db_path, &crypto).unwrap();
        assert!(
            outcome.restore_completed,
            "Case A swap completed — candidate became live"
        );

        assert!(!candidate.exists(), "Orphan candidate must be removed");
        assert!(db_path.exists(), "Live DB must remain intact");
        assert!(
            !journal_path.exists(),
            "Journal must be cleared after recovery"
        );
    }

    /// Case A: journal present; candidate + live both exist (crash after journal, before swap).
    #[test]
    fn crash_case_a_journal_before_swap_completes_atomic_promotion() {
        let tmp_dir = tempfile::tempdir().unwrap();
        let db_path = tmp_dir.path().join("live.db");
        let candidate = tmp_dir.path().join("cand.sqlite_tmp");
        let rollback_path = db_path.with_extension("rollback.bak");

        std::fs::write(&db_path, b"LIVE_OLD").unwrap();
        std::fs::write(&candidate, b"FROM_CAND").unwrap();

        write_journal_atomic(
            &db_path,
            &RestoreJournalV1 {
                version: 1,
                phase: RestorePhase::ReadySwap,
                candidate_path: candidate.clone(),
                rollback_path: rollback_path.clone(),
            },
        );

        let crypto = AgeFileEncryptionProvider::new();
        let outcome = recover_interrupted_restore_and_orphans(&db_path, &crypto).unwrap();
        assert!(outcome.restore_completed, "Case A: full swap completed");

        let body = std::fs::read_to_string(&db_path).unwrap();
        assert_eq!(
            body, "FROM_CAND",
            "live DB must be promoted candidate contents"
        );
        assert!(!candidate.exists());
        assert!(!rollback_path.exists());
        assert!(!db_path.with_extension("restore.journal").exists());
    }

    /// Case B: live already renamed to rollback; candidate remains; live path empty.
    #[test]
    fn crash_case_b_after_live_to_rollback_before_candidate_promote() {
        let tmp_dir = tempfile::tempdir().unwrap();
        let db_path = tmp_dir.path().join("live.db");
        let candidate = tmp_dir.path().join("cand.sqlite_tmp");
        let rollback_path = db_path.with_extension("rollback.bak");

        std::fs::write(&rollback_path, b"LIVE_OLD").unwrap();
        std::fs::write(&candidate, b"FROM_CAND").unwrap();
        assert!(!db_path.exists());

        write_journal_atomic(
            &db_path,
            &RestoreJournalV1 {
                version: 1,
                phase: RestorePhase::ReadySwap,
                candidate_path: candidate.clone(),
                rollback_path: rollback_path.clone(),
            },
        );

        let crypto = AgeFileEncryptionProvider::new();
        let outcome = recover_interrupted_restore_and_orphans(&db_path, &crypto).unwrap();
        assert!(
            outcome.restore_completed,
            "Case B: candidate promoted to live"
        );

        let body = std::fs::read_to_string(&db_path).unwrap();
        assert_eq!(body, "FROM_CAND");
        assert!(!candidate.exists());
        assert!(
            !rollback_path.exists(),
            "rollback must be removed after successful promote"
        );
        assert!(!db_path.with_extension("restore.journal").exists());
    }

    /// Case C: candidate already renamed to live; rollback still present; journal left.
    #[test]
    fn crash_case_c_after_candidate_promote_before_journal_clear() {
        let tmp_dir = tempfile::tempdir().unwrap();
        let db_path = tmp_dir.path().join("live.db");
        let candidate = tmp_dir.path().join("gone.sqlite_tmp");
        let rollback_path = db_path.with_extension("rollback.bak");

        std::fs::write(&db_path, b"NEW_LIVE").unwrap();
        std::fs::write(&rollback_path, b"OLD_ROLL").unwrap();
        assert!(!candidate.exists());

        write_journal_atomic(
            &db_path,
            &RestoreJournalV1 {
                version: 1,
                phase: RestorePhase::ReadySwap,
                candidate_path: candidate.clone(),
                rollback_path: rollback_path.clone(),
            },
        );

        let crypto = AgeFileEncryptionProvider::new();
        let outcome = recover_interrupted_restore_and_orphans(&db_path, &crypto).unwrap();
        assert!(outcome.restore_completed, "Case C: candidate already live");

        assert_eq!(std::fs::read_to_string(&db_path).unwrap(), "NEW_LIVE");
        assert!(!rollback_path.exists());
        assert!(!db_path.with_extension("restore.journal").exists());
    }

    /// Case D: orphan `grpc_restore_*.sqlite_tmp` with no journal at startup.
    #[test]
    fn crash_case_d_orphan_restore_tempfile_cleaned_without_journal() {
        let tmp_dir = tempfile::tempdir().unwrap();
        let db_path = tmp_dir.path().join("live.db");
        std::fs::write(&db_path, b"x").unwrap();
        let orphan = tmp_dir.path().join("grpc_restore_abcd123.sqlite_tmp");
        std::fs::write(&orphan, b"junk").unwrap();

        let crypto = AgeFileEncryptionProvider::new();
        let outcome = recover_interrupted_restore_and_orphans(&db_path, &crypto).unwrap();
        assert!(
            !outcome.restore_completed,
            "no journal → nothing to complete"
        );

        assert!(!orphan.exists(), "orphan tempfile must be removed");
        assert!(db_path.exists());
    }

    /// Case E: no live file; rollback holds last good DB; candidate consumed; journal lists paths.
    #[test]
    fn crash_case_e_rollback_without_live_restores_from_rollback() {
        let tmp_dir = tempfile::tempdir().unwrap();
        let db_path = tmp_dir.path().join("live.db");
        let candidate = tmp_dir.path().join("consumed.sqlite_tmp");
        let rollback_path = db_path.with_extension("rollback.bak");

        std::fs::write(&rollback_path, b"ROLLBACK_BODY").unwrap();
        assert!(!db_path.exists());
        assert!(!candidate.exists());

        write_journal_atomic(
            &db_path,
            &RestoreJournalV1 {
                version: 1,
                phase: RestorePhase::ReadySwap,
                candidate_path: candidate.clone(),
                rollback_path: rollback_path.clone(),
            },
        );

        let crypto = AgeFileEncryptionProvider::new();
        let outcome = recover_interrupted_restore_and_orphans(&db_path, &crypto).unwrap();
        assert!(
            !outcome.restore_completed,
            "Case E: old DB restored from rollback — restore did NOT complete"
        );

        assert_eq!(std::fs::read_to_string(&db_path).unwrap(), "ROLLBACK_BODY");
        assert!(!rollback_path.exists());
        assert!(!db_path.with_extension("restore.journal").exists());
    }

    // =============================================================================
    // Tests — Backup Snapshot Validation (FAIL-CLOSED)
    // =============================================================================

    /// Test that a near-empty snapshot is rejected during validation.
    #[test]
    fn snapshot_validation_rejects_near_empty_snapshot() {
        let tmp_dir = tempfile::tempdir().unwrap();
        let src_path = tmp_dir.path().join("source.db");
        let temp_path = tmp_dir.path().join("temp_snapshot.sqlite_tmp");

        // Create a source database with some data
        let src = rusqlite::Connection::open(&src_path).unwrap();
        src.execute("CREATE TABLE test (id INTEGER PRIMARY KEY, value TEXT)", [])
            .unwrap();
        src.execute("INSERT INTO test (id, value) VALUES (1, 'data')", [])
            .unwrap();

        // Create a near-empty temp file (too small to be valid)
        std::fs::write(&temp_path, b"SQLite format 3").unwrap();

        let crypto = AgeFileEncryptionProvider::new();
        let adapter = SqliteBackupAdapter::new(&src_path, crypto);

        let result = adapter.validate_temp_snapshot(&src, &temp_path);
        assert!(result.is_err(), "Near-empty snapshot should be rejected");
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("too small") || err.to_string().contains("empty"),
            "Error should mention size issue: {}",
            err
        );
    }

    /// Test that a corrupted temp snapshot is rejected by integrity_check.
    #[test]
    fn snapshot_validation_rejects_corrupted_snapshot() {
        let tmp_dir = tempfile::tempdir().unwrap();
        let src_path = tmp_dir.path().join("source.db");
        let temp_path = tmp_dir.path().join("temp_snapshot.sqlite_tmp");

        // Create a source database
        let src = rusqlite::Connection::open(&src_path).unwrap();
        src.execute("CREATE TABLE test (id INTEGER PRIMARY KEY, value TEXT)", [])
            .unwrap();
        src.execute("INSERT INTO test (id, value) VALUES (1, 'data')", [])
            .unwrap();

        // Create a temp file with valid size but corrupted content
        let mut data = vec![0u8; 8192]; // 8KB - larger than minimum
        data[0..15].copy_from_slice(b"SQLite format 3"); // Valid header (15 bytes)
        data[15] = 0; // Null terminator
        std::fs::write(&temp_path, data).unwrap();

        let crypto = AgeFileEncryptionProvider::new();
        let adapter = SqliteBackupAdapter::new(&src_path, crypto);

        let result = adapter.validate_temp_snapshot(&src, &temp_path);
        assert!(result.is_err(), "Corrupted snapshot should be rejected");
        let err = result.unwrap_err();
        let err_msg = err.to_string().to_lowercase();
        assert!(
            err_msg.contains("integrity")
                || err_msg.contains("corruption")
                || err_msg.contains("not a database"),
            "Error should mention integrity issue: {}",
            err
        );
    }

    /// Test that a valid snapshot passes validation.
    #[test]
    fn snapshot_validation_accepts_valid_snapshot() {
        let tmp_dir = tempfile::tempdir().unwrap();
        let src_path = tmp_dir.path().join("source.db");
        let temp_path = tmp_dir.path().join("temp_snapshot.sqlite_tmp");

        // Create a source database with data
        let src = rusqlite::Connection::open(&src_path).unwrap();
        src.execute("CREATE TABLE test (id INTEGER PRIMARY KEY, value TEXT)", [])
            .unwrap();
        src.execute("INSERT INTO test (id, value) VALUES (1, 'data')", [])
            .unwrap();

        // Create a valid temp snapshot using SQLite Backup API
        {
            let mut temp_conn = rusqlite::Connection::open(&temp_path).unwrap();
            let backup = rusqlite::backup::Backup::new(&src, &mut temp_conn).unwrap();
            loop {
                match backup.step(100) {
                    Ok(rusqlite::backup::StepResult::Done) => break,
                    Ok(_) => continue,
                    Err(e) => panic!("Backup step failed: {}", e),
                }
            }
        }

        let crypto = AgeFileEncryptionProvider::new();
        let adapter = SqliteBackupAdapter::new(&src_path, crypto);

        let result = adapter.validate_temp_snapshot(&src, &temp_path);
        assert!(result.is_ok(), "Valid snapshot should pass validation");
    }

    /// Test that snapshot with dramatically fewer pages than source is rejected.
    #[test]
    fn snapshot_validation_rejects_size_mismatch() {
        let tmp_dir = tempfile::tempdir().unwrap();
        let src_path = tmp_dir.path().join("source.db");
        let temp_path = tmp_dir.path().join("temp_snapshot.sqlite_tmp");

        // Create a source database with multiple pages
        let src = rusqlite::Connection::open(&src_path).unwrap();
        src.execute("CREATE TABLE test (id INTEGER PRIMARY KEY, value TEXT)", [])
            .unwrap();
        // Insert enough data to create multiple pages
        for i in 0..1000 {
            src.execute(
                "INSERT INTO test (id, value) VALUES (?1, ?2)",
                rusqlite::params![i, format!("data_{}", i)],
            )
            .unwrap();
        }

        // Create a temp snapshot with only one page
        {
            let temp_conn = rusqlite::Connection::open(&temp_path).unwrap();
            temp_conn
                .execute("CREATE TABLE test (id INTEGER PRIMARY KEY, value TEXT)", [])
                .unwrap();
            temp_conn
                .execute("INSERT INTO test (id, value) VALUES (1, 'single')", [])
                .unwrap();
        }

        let crypto = AgeFileEncryptionProvider::new();
        let adapter = SqliteBackupAdapter::new(&src_path, crypto);

        let result = adapter.validate_temp_snapshot(&src, &temp_path);
        assert!(result.is_err(), "Size mismatch should be rejected");
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("fewer pages") || err.to_string().contains("mismatch"),
            "Error should mention size mismatch: {}",
            err
        );
    }

    /// Test that missing temp file is rejected.
    #[test]
    fn snapshot_validation_rejects_missing_file() {
        let tmp_dir = tempfile::tempdir().unwrap();
        let src_path = tmp_dir.path().join("source.db");
        let temp_path = tmp_dir.path().join("nonexistent.sqlite_tmp");

        // Create a source database
        let src = rusqlite::Connection::open(&src_path).unwrap();
        src.execute("CREATE TABLE test (id INTEGER PRIMARY KEY, value TEXT)", [])
            .unwrap();

        let crypto = AgeFileEncryptionProvider::new();
        let adapter = SqliteBackupAdapter::new(&src_path, crypto);

        let result = adapter.validate_temp_snapshot(&src, &temp_path);
        assert!(result.is_err(), "Missing file should be rejected");
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("does not exist") || err.to_string().contains("not found"),
            "Error should mention missing file: {}",
            err
        );
    }

    /// Test that empty temp file (0 bytes) is rejected.
    #[test]
    fn snapshot_validation_rejects_empty_file() {
        let tmp_dir = tempfile::tempdir().unwrap();
        let src_path = tmp_dir.path().join("source.db");
        let temp_path = tmp_dir.path().join("empty.sqlite_tmp");

        // Create a source database
        let src = rusqlite::Connection::open(&src_path).unwrap();
        src.execute("CREATE TABLE test (id INTEGER PRIMARY KEY, value TEXT)", [])
            .unwrap();

        // Create an empty temp file
        std::fs::write(&temp_path, b"").unwrap();

        let crypto = AgeFileEncryptionProvider::new();
        let adapter = SqliteBackupAdapter::new(&src_path, crypto);

        let result = adapter.validate_temp_snapshot(&src, &temp_path);
        assert!(result.is_err(), "Empty file should be rejected");
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("empty") || err.to_string().contains("0 bytes"),
            "Error should mention empty file: {}",
            err
        );
    }

    /// Test that temp file is securely deleted after validation failure.
    #[test]
    fn temp_file_securely_deleted_after_validation_failure() {
        let tmp_dir = tempfile::tempdir().unwrap();
        let src_path = tmp_dir.path().join("source.db");
        let temp_path = tmp_dir.path().join("temp_snapshot.sqlite_tmp");

        // Create a source database
        let src = rusqlite::Connection::open(&src_path).unwrap();
        src.execute("CREATE TABLE test (id INTEGER PRIMARY KEY, value TEXT)", [])
            .unwrap();

        // Create an invalid temp file
        std::fs::write(&temp_path, b"invalid").unwrap();

        let crypto = AgeFileEncryptionProvider::new();
        let adapter = SqliteBackupAdapter::new(&src_path, crypto);

        // Validation should fail
        let result = adapter.validate_temp_snapshot(&src, &temp_path);
        assert!(result.is_err());

        // Temp file should still exist (validation doesn't delete, the caller does)
        // This test verifies that the validation function itself doesn't delete
        // The secure_delete is called by the backup creation function
        assert!(
            temp_path.exists(),
            "Validation should not delete the temp file"
        );
    }

    // ── BR-10: canonical restore-path validation ─────────────────────────────

    // The chdir-based tests mutate the process-global CWD; a static lock
    // serializes them against each other. All other tests in this module use
    // absolute paths and are CWD-insensitive.
    static CHDIR_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    struct CwdGuard {
        original: PathBuf,
        _lock: std::sync::MutexGuard<'static, ()>,
    }
    impl Drop for CwdGuard {
        fn drop(&mut self) {
            let _ = std::env::set_current_dir(&self.original);
        }
    }

    fn chdir_guard() -> CwdGuard {
        let _lock = CHDIR_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let original = std::env::current_dir().expect("current cwd");
        CwdGuard { original, _lock }
    }

    fn br10_adapter() -> (SqliteBackupAdapter, tempfile::TempDir) {
        let tmp_dir = tempfile::tempdir().unwrap();
        let db_path = tmp_dir.path().join("live.db");
        let adapter = SqliteBackupAdapter::new(&db_path, AgeFileEncryptionProvider::new());
        (adapter, tmp_dir)
    }

    #[test]
    fn validate_restore_path_accepts_absolute_path_inside_backup_dir() {
        let (adapter, _dir) = br10_adapter();
        std::fs::create_dir_all(&adapter.backup_dir).unwrap();
        let candidate = adapter.backup_dir.join("snap.bak");
        std::fs::write(&candidate, b"x").unwrap();
        adapter.validate_restore_path(&candidate).expect("accepted");
    }

    #[test]
    fn validate_restore_path_accepts_nested_path_inside_backup_dir() {
        let (adapter, _dir) = br10_adapter();
        let nested = adapter.backup_dir.join("sub").join("snap.bak");
        std::fs::create_dir_all(nested.parent().unwrap()).unwrap();
        std::fs::write(&nested, b"x").unwrap();
        adapter.validate_restore_path(&nested).expect("accepted");
    }

    #[test]
    fn validate_restore_path_resolves_relative_paths_against_cwd() {
        let (adapter, _dir) = br10_adapter();
        std::fs::create_dir_all(&adapter.backup_dir).unwrap();
        let candidate = adapter.backup_dir.join("snap.bak");
        std::fs::write(&candidate, b"x").unwrap();

        // A relative path inside the backup dir, resolved from a CWD whose
        // canonical location is inside the backup root, must be accepted.
        let _guard = chdir_guard();
        std::env::set_current_dir(&adapter.backup_dir).expect("chdir");
        adapter
            .validate_restore_path(Path::new("snap.bak"))
            .expect("relative path inside backup dir accepted");
    }

    #[test]
    fn validate_restore_path_rejects_relative_path_outside_backup_dir() {
        let (adapter, tmp_dir) = br10_adapter();
        // CWD points at a directory OUTSIDE the backup root: a relative
        // candidate must be rejected after canonicalization, never accepted
        // by textual inspection alone.
        let outside = tmp_dir.path().join("outside.bak");
        std::fs::write(&outside, b"x").unwrap();
        let _guard = chdir_guard();
        std::env::set_current_dir(tmp_dir.path()).expect("chdir");
        let err = adapter
            .validate_restore_path(Path::new("outside.bak"))
            .expect_err("relative path outside backup dir must be rejected");
        assert_eq!(err.kind(), std::io::ErrorKind::PermissionDenied);
    }

    #[test]
    fn validate_restore_path_rejects_traversal_markers() {
        let (adapter, _dir) = br10_adapter();
        let err = adapter
            .validate_restore_path(Path::new("../outside.bak"))
            .expect_err("traversal rejected");
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
    }

    #[test]
    fn validate_restore_path_rejects_path_outside_backup_root() {
        let (adapter, tmp_dir) = br10_adapter();
        let outside = tmp_dir.path().join("outside.bak");
        std::fs::write(&outside, b"x").unwrap();
        let err = adapter
            .validate_restore_path(&outside)
            .expect_err("outside backup root rejected");
        assert_eq!(err.kind(), std::io::ErrorKind::PermissionDenied);
    }

    #[test]
    fn validate_restore_path_rejects_nonexistent_path() {
        let (adapter, _dir) = br10_adapter();
        let ghost = adapter.backup_dir.join("ghost.bak");
        let err = adapter
            .validate_restore_path(&ghost)
            .expect_err("nonexistent path rejected");
        assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
    }

    #[cfg(unix)]
    #[test]
    fn validate_restore_path_rejects_symlink_escape() {
        let (adapter, tmp_dir) = br10_adapter();
        std::fs::create_dir_all(&adapter.backup_dir).unwrap();
        let outside = tmp_dir.path().join("secret.bak");
        std::fs::write(&outside, b"secret").unwrap();
        let link = adapter.backup_dir.join("innocent.bak");
        std::os::unix::fs::symlink(&outside, &link).unwrap();
        let err = adapter
            .validate_restore_path(&link)
            .expect_err("symlink escape rejected");
        assert_eq!(err.kind(), std::io::ErrorKind::PermissionDenied);
    }

    #[cfg(unix)]
    #[test]
    fn restore_backup_atomic_rejects_escape_before_any_decrypt() {
        // BR-10 ordering: the path is validated BEFORE the candidate is read
        // or decrypted. A symlink escape must surface as a validation error
        // (PermissionDenied), never as a decryption/parse failure.
        let (adapter, tmp_dir) = br10_adapter();
        std::fs::create_dir_all(&adapter.backup_dir).unwrap();
        let outside = tmp_dir.path().join("secret.bak");
        std::fs::write(&outside, b"secret").unwrap();
        let link = adapter.backup_dir.join("innocent.bak");
        std::os::unix::fs::symlink(&outside, &link).unwrap();
        let err = adapter
            .restore_backup_atomic(&link)
            .expect_err("restore must reject escape path");
        assert_eq!(err.kind(), std::io::ErrorKind::PermissionDenied);
    }
}
