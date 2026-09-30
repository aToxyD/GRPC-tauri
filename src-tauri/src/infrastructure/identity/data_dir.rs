//! Identity data-directory resolution (ADR-0062).
//!
//! Single owner of the on-disk location of **identity and secret state**:
//! `node_identity.key`, its staged rotation slot `node_identity.key.pending`,
//! `.adminkey`, and `appkey.age`. Operational data (`GRPC/logs`, backup
//! discovery) and the SQLite database (`GRPC_DB_PATH`) are explicitly **not**
//! covered (ADR-0062 §1 and §7) — this is not an application-data resolver.
//!
//! Precedence (ADR-0062 §2): `GRPC_IDENTITY_DATA_DIR` when it holds an absolute
//! path, otherwise the platform data directory `dirs::data_dir()/GRPC`. Empty
//! and whitespace-only values are treated as unset. A relative value is
//! rejected; the current working directory is never used as a base.
//!
//! The override is honored in **every** profile and is never gated on
//! `GRPC_ENV` (ADR-0062 §4). The resolved directory is stable for the process
//! lifetime (ADR-0062 §3), and the cross-directory shadow guard (ADR-0062 §5)
//! runs on every resolution, before any identity read or write.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::errors::{AppError, AppResult};
use crate::infrastructure::security::appkey_store::APPKEY_FILE_NAME;

use super::adminkey_provider::{ADMINKEY_FILE_NAME, GRPC_DATA_DIR};
use super::node_key_store::{NODE_KEY_FILE_NAME, NODE_KEY_PENDING_FILE_NAME};

/// Environment variable holding an absolute identity data-directory override.
pub const IDENTITY_DATA_DIR_ENV: &str = "GRPC_IDENTITY_DATA_DIR";

/// The normative identity files for ADR-0062 §5: the active node signing key,
/// the staged rotation key, the portable operator key, and the passphrase-
/// protected App-Key store. No other file participates in the guard — `*.tmp`
/// staging files, `grpc.db`, `backups/`, and `logs/` are excluded.
pub const IDENTITY_FILE_NAMES: [&str; 4] = [
    NODE_KEY_FILE_NAME,
    NODE_KEY_PENDING_FILE_NAME,
    ADMINKEY_FILE_NAME,
    APPKEY_FILE_NAME,
];

/// Process-lifetime cache of the first successful resolution (ADR-0062 §3).
/// Established repository pattern: `static` + `Mutex` (see
/// `infrastructure/security/mod.rs:32`). Only a *successful* resolution is
/// cached, so a transient failure never freezes a directory; there is
/// deliberately no production reset capability.
static RESOLVED_IDENTITY_DATA_DIR: Mutex<Option<PathBuf>> = Mutex::new(None);

/// Resolve the identity data directory, stable for the process lifetime.
///
/// This is the single entry point every in-scope consumer uses. It never
/// resolves against the current working directory and never weakens its
/// contract in a non-production profile.
pub fn identity_data_dir() -> AppResult<PathBuf> {
    let mut cached = RESOLVED_IDENTITY_DATA_DIR.lock().map_err(|e| {
        AppError::Internal(format!("Failed to lock the identity data dir cache: {e}"))
    })?;
    if let Some(dir) = cached.as_ref() {
        return Ok(dir.clone());
    }
    let resolved = resolve_identity_data_dir()?;
    *cached = Some(resolved.clone());
    Ok(resolved)
}

/// Uncached resolution: reads the process environment and the platform data
/// directory. Reached only through [`identity_data_dir`].
fn resolve_identity_data_dir() -> AppResult<PathBuf> {
    let raw = std::env::var(IDENTITY_DATA_DIR_ENV).ok();
    let platform_dir = dirs::data_dir().map(|dir| dir.join(GRPC_DATA_DIR));
    resolve_from(raw.as_deref(), platform_dir)
}

/// The ADR-0062 §2 contract, parameterized so it is deterministic under test.
/// `platform_dir` is the platform default (`dirs::data_dir()/GRPC`) when the
/// platform can resolve one.
fn resolve_from(raw: Option<&str>, platform_dir: Option<PathBuf>) -> AppResult<PathBuf> {
    // Empty and whitespace-only are "unset": `PathBuf::from("")` is never built.
    let Some(value) = raw.map(str::trim).filter(|value| !value.is_empty()) else {
        return platform_dir
            .ok_or_else(|| AppError::Internal("Cannot resolve system data directory".into()));
    };

    let override_dir = PathBuf::from(value);
    if !override_dir.is_absolute() {
        return Err(AppError::Configuration(format!(
            "{IDENTITY_DATA_DIR_ENV} must hold an absolute directory path"
        )));
    }

    // The §5 guard needs the platform default to detect cross-directory
    // shadowing. If the platform default cannot be resolved the check cannot
    // run, so the override is refused rather than trusted: an inspection that
    // cannot be performed is a hard error, never a silent "nothing to find".
    let default_dir = platform_dir.ok_or_else(|| {
        AppError::Configuration(format!(
            "{IDENTITY_DATA_DIR_ENV} cannot be verified because the platform data directory is unavailable"
        ))
    })?;
    ensure_no_cross_directory_shadow(&override_dir, default_dir.as_path())?;

    // The directory is not a secret; no secret value is ever logged (§4).
    log::warn!(
        target: "grpc::identity",
        "{IDENTITY_DATA_DIR_ENV} is set — identity and secret state resolves to the operator-selected directory: {}",
        override_dir.display()
    );
    Ok(override_dir)
}

/// ADR-0062 §5 — fail closed when the overridden directory holds none of the
/// identity files while the platform default holds at least one, so a wrong
/// override cannot present a fresh, unprovisioned node.
///
/// This is a **point-in-time** inspection performed before any identity access.
/// It performs no content comparison and no cryptographic cross-check between
/// the two directories, and it introduces no cross-process lock (§5).
fn ensure_no_cross_directory_shadow(override_dir: &Path, default_dir: &Path) -> AppResult<()> {
    let default_populated = directory_holds_identity_files(default_dir)?;
    let override_populated = directory_holds_identity_files(override_dir)?;
    if !default_populated || override_populated {
        return Ok(());
    }
    Err(AppError::Configuration(format!(
        "{IDENTITY_DATA_DIR_ENV} resolves to a directory holding none of the GRPC identity files while the platform data directory holds some — refusing to report this node as unprovisioned"
    )))
}

/// Outcome of one filesystem probe.
#[derive(Debug)]
enum Probe {
    Missing,
    Found(fs::Metadata),
}

/// Classify a filesystem probe. `NotFound` is the only benign outcome; every
/// other I/O error is a hard failure and is never read as "empty" (§5 case 7).
fn classify_probe(outcome: io::Result<fs::Metadata>) -> AppResult<Probe> {
    match outcome {
        Ok(metadata) => Ok(Probe::Found(metadata)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Probe::Missing),
        Err(e) => Err(AppError::Io(e)),
    }
}

/// Type-check `dir` and report whether it holds any normative identity file.
///
/// A missing directory is not an error (§5 case 6); a path that exists as
/// something other than a directory is (§5 case 8); an identity file path that
/// exists as a link or any non-regular-file type is (§5 case 9).
fn directory_holds_identity_files(dir: &Path) -> AppResult<bool> {
    match classify_probe(fs::metadata(dir))? {
        Probe::Missing => return Ok(false),
        Probe::Found(metadata) if metadata.is_dir() => {}
        Probe::Found(_) => {
            return Err(AppError::Configuration(format!(
                "{IDENTITY_DATA_DIR_ENV} path is not a directory"
            )))
        }
    }

    let mut present = false;
    for name in IDENTITY_FILE_NAMES {
        // `symlink_metadata` so a link is detected as a link rather than
        // silently followed to whatever it points at (§5 case 9).
        match classify_probe(fs::symlink_metadata(dir.join(name)))? {
            Probe::Missing => {}
            Probe::Found(metadata) if metadata.file_type().is_symlink() => {
                return Err(AppError::Configuration(format!(
                    "{IDENTITY_DATA_DIR_ENV} holds an identity file path that is a link"
                )))
            }
            Probe::Found(metadata) if metadata.is_file() => present = true,
            Probe::Found(_) => {
                return Err(AppError::Configuration(format!(
                    "{IDENTITY_DATA_DIR_ENV} holds an identity file path that is not a regular file"
                )))
            }
        }
    }
    Ok(present)
}

#[cfg(test)]
mod tests {
    use super::*;

    use tempfile::TempDir;

    use crate::infrastructure::identity::adminkey_provider::AdminKeyProvider;
    use crate::infrastructure::identity::node_key_store::NodeKeyStore;
    use crate::infrastructure::security::appkey_store::AppKeyStore;
    use crate::infrastructure::security::test_support::lock_security_test_env;

    fn platform_data_dir() -> PathBuf {
        dirs::data_dir()
            .map(|dir| dir.join(GRPC_DATA_DIR))
            .expect("platform data directory is resolvable in this environment")
    }

    fn seed_identity_file(dir: &Path, name: &str) {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join(name), b"identity-material").unwrap();
    }

    // --- §2 environment contract -------------------------------------------

    #[test]
    fn unset_override_resolves_to_the_platform_data_directory() {
        let expected = platform_data_dir();
        assert_eq!(
            resolve_from(None, Some(expected.clone())).unwrap(),
            expected
        );
    }

    #[test]
    fn empty_and_whitespace_values_are_treated_as_unset() {
        let expected = platform_data_dir();
        for raw in ["", " ", "\t", "\n", "  \t\n  "] {
            let resolved = resolve_from(Some(raw), Some(expected.clone())).unwrap_or_else(|e| {
                panic!("value {raw:?} must resolve to the platform default: {e}")
            });
            assert_eq!(
                resolved, expected,
                "value {raw:?} must not build an empty path"
            );
        }
    }

    #[test]
    fn relative_override_is_rejected_as_a_configuration_error() {
        let platform = platform_data_dir();
        for raw in ["identity", "./identity", "GRPC/identity", r"C:identity"] {
            let err = resolve_from(Some(raw), Some(platform.clone()))
                .expect_err("a relative override must be rejected");
            assert!(
                matches!(err, AppError::Configuration(_)),
                "value {raw:?} produced {err:?}"
            );
        }
    }

    #[test]
    fn absolute_override_is_used_verbatim() {
        let platform = TempDir::new().unwrap();
        let target = TempDir::new().unwrap();
        let expected = target.path().to_path_buf();
        let resolved = resolve_from(
            Some(expected.to_str().unwrap()),
            Some(platform.path().to_path_buf()),
        )
        .unwrap();
        assert_eq!(resolved, expected);
    }

    #[test]
    fn an_unverifiable_override_is_refused_rather_than_trusted() {
        let target = TempDir::new().unwrap();
        let err = resolve_from(Some(target.path().to_str().unwrap()), None)
            .expect_err("without a platform default the guard cannot run");
        assert!(
            matches!(err, AppError::Configuration(_)),
            "expected a configuration error, got {err:?}"
        );
    }

    // --- §5 shadow guard ----------------------------------------------------

    #[test]
    fn first_run_with_nothing_in_either_directory_is_allowed() {
        let platform = TempDir::new().unwrap();
        let target = TempDir::new().unwrap();
        ensure_no_cross_directory_shadow(target.path(), platform.path()).unwrap();
    }

    #[test]
    fn an_empty_override_against_a_populated_default_fails_closed() {
        let platform = TempDir::new().unwrap();
        let target = TempDir::new().unwrap();
        seed_identity_file(platform.path(), ADMINKEY_FILE_NAME);
        let err = ensure_no_cross_directory_shadow(target.path(), platform.path())
            .expect_err("an empty override against existing identity state must fail closed");
        assert!(
            matches!(err, AppError::Configuration(_)),
            "expected a configuration error, got {err:?}"
        );
    }

    #[test]
    fn the_guard_is_not_weakened_by_the_e2e_profile() {
        let _env = lock_security_test_env();
        let platform = TempDir::new().unwrap();
        let target = TempDir::new().unwrap();
        seed_identity_file(platform.path(), NODE_KEY_FILE_NAME);
        std::env::set_var("GRPC_ENV", "test");
        let outcome = ensure_no_cross_directory_shadow(target.path(), platform.path());
        std::env::remove_var("GRPC_ENV");
        assert!(
            matches!(outcome, Err(AppError::Configuration(_))),
            "the guard applies in every profile, got {outcome:?}"
        );
    }

    #[test]
    fn the_guard_compares_no_content_across_directories() {
        let platform = TempDir::new().unwrap();
        let target = TempDir::new().unwrap();
        seed_identity_file(platform.path(), ADMINKEY_FILE_NAME);
        seed_identity_file(target.path(), APPKEY_FILE_NAME);
        // §5 cases 3 and 4: each store is read from its own resolved directory.
        ensure_no_cross_directory_shadow(target.path(), platform.path()).unwrap();
    }

    #[test]
    fn a_single_store_in_the_overridden_directory_is_allowed() {
        let platform = TempDir::new().unwrap();
        let target = TempDir::new().unwrap();
        seed_identity_file(target.path(), NODE_KEY_FILE_NAME);
        ensure_no_cross_directory_shadow(target.path(), platform.path()).unwrap();
    }

    #[test]
    fn a_missing_overridden_directory_is_allowed() {
        let platform = TempDir::new().unwrap();
        let parent = TempDir::new().unwrap();
        let target = parent.path().join("not-created-yet");
        ensure_no_cross_directory_shadow(&target, platform.path()).unwrap();
    }

    #[test]
    fn a_non_directory_identity_path_is_refused() {
        let platform = TempDir::new().unwrap();
        let parent = TempDir::new().unwrap();
        let target = parent.path().join("GRPC");
        fs::write(&target, b"not a directory").unwrap();
        let err = ensure_no_cross_directory_shadow(&target, platform.path())
            .expect_err("a non-directory identity path is type ambiguity");
        assert!(
            matches!(err, AppError::Configuration(_)),
            "expected a configuration error, got {err:?}"
        );
    }

    #[test]
    fn an_identity_file_path_of_an_unexpected_type_is_refused() {
        let platform = TempDir::new().unwrap();
        let target = TempDir::new().unwrap();
        fs::create_dir(target.path().join(ADMINKEY_FILE_NAME)).unwrap();
        let err = ensure_no_cross_directory_shadow(target.path(), platform.path())
            .expect_err("a directory where an identity file is expected is type ambiguity");
        assert!(
            matches!(err, AppError::Configuration(_)),
            "expected a configuration error, got {err:?}"
        );
    }

    #[test]
    fn a_real_io_error_is_never_read_as_empty() {
        let denied = classify_probe(Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "denied",
        )));
        assert!(
            matches!(denied, Err(AppError::Io(_))),
            "a real I/O error must fail closed, got {denied:?}"
        );
        let absent = classify_probe(Err(io::Error::new(io::ErrorKind::NotFound, "absent")));
        assert!(
            matches!(absent, Ok(Probe::Missing)),
            "only NotFound is benign, got {absent:?}"
        );
    }

    // --- §3 process-lifetime stability and cross-store coherence ------------

    #[test]
    fn resolution_is_stable_for_the_process_lifetime() {
        let _env = lock_security_test_env();
        // The first value is the platform default so the §5 guard is inert; the
        // second is a different absolute directory. The invariant under test is
        // that the first resolution wins for the whole process.
        let platform = platform_data_dir();
        let decoy = TempDir::new().unwrap();
        let decoy_value = decoy.path().to_str().unwrap().to_string();

        std::env::set_var(IDENTITY_DATA_DIR_ENV, platform.as_os_str());
        let pinned = identity_data_dir().expect("the platform default is always resolvable");

        std::env::set_var(IDENTITY_DATA_DIR_ENV, &decoy_value);
        assert_eq!(
            identity_data_dir().unwrap(),
            pinned,
            "a later environment change must not move the resolved directory"
        );

        std::env::remove_var(IDENTITY_DATA_DIR_ENV);
        assert_eq!(
            identity_data_dir().unwrap(),
            pinned,
            "unsetting the variable must not move the resolved directory either"
        );
    }

    #[test]
    fn every_identity_store_resolves_to_the_same_directory() {
        let _env = lock_security_test_env();
        let resolved = identity_data_dir().unwrap();
        for parent in [
            NodeKeyStore::new(resolved.clone())
                .file_path()
                .parent()
                .map(Path::to_path_buf),
            AdminKeyProvider::new(resolved.clone())
                .file_path()
                .parent()
                .map(Path::to_path_buf),
            AppKeyStore::new(resolved.clone())
                .file_path()
                .parent()
                .map(Path::to_path_buf),
        ] {
            assert_eq!(parent.as_deref(), Some(resolved.as_path()));
        }
    }

    #[test]
    fn security_status_store_path_follows_the_resolved_directory() {
        // Targets the hub bypass at infrastructure/security/mod.rs:134.
        let _env = lock_security_test_env();
        let expected = AppKeyStore::new(identity_data_dir().unwrap()).file_path();
        let status = crate::infrastructure::security::app_key_status().unwrap();
        assert_eq!(status.store_path, expected.display().to_string());
    }

    #[test]
    fn operational_directories_ignore_the_identity_override() {
        let _env = lock_security_test_env();
        let platform = platform_data_dir();
        let decoy = TempDir::new().unwrap();
        let decoy_value = decoy.path().to_str().unwrap().to_string();

        std::env::set_var(IDENTITY_DATA_DIR_ENV, &decoy_value);
        let log_dir = crate::infrastructure::logging::resolve_log_dir();
        std::env::remove_var(IDENTITY_DATA_DIR_ENV);

        assert_eq!(
            log_dir,
            Some(platform.join("logs")),
            "site D is operational and must stay on the platform data directory"
        );
    }
}
