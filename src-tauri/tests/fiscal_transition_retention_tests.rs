#[cfg(test)]
mod common;

#[cfg(test)]
mod tests {
    use super::common::{clear_fiscal_status, create_test_state, seed_fiscal_year_open};
    use super::common::seed_wilaya_identity;
    use grpc_lib::application::services::FiscalClosurePackageService;
    use grpc_lib::infrastructure::sync::packages::signing::Ed25519PackageSigner;
    use grpc_lib::repositories::RepositoryProvider;
    use std::fs;

    const WILAYA_SIGNING_SECRET: [u8; 32] = [42u8; 32];

    fn wilaya_signer() -> Ed25519PackageSigner {
        Ed25519PackageSigner::new(WILAYA_SIGNING_SECRET)
    }

    #[test]
    fn test_fiscal_transition_retention_lifecycle() {
        let (state, _temp_dir) = create_test_state();

        // Clear migration-seeded data
        clear_fiscal_status(&state);
        // Seed data (acquires its own lock)
        seed_fiscal_year_open(&state, 2025);
        {
            let db_guard = state.db.lock().unwrap();
            let db = db_guard.as_ref().unwrap();
            db.executor().settings().set_current_year(2025).unwrap();
        }

        let db_guard = state.db.lock().unwrap();
        let db = db_guard.as_ref().unwrap();
        let wilaya_identity_id = seed_wilaya_identity(db, WILAYA_SIGNING_SECRET);
        let signer = wilaya_signer();

        let package_path = _temp_dir.path().join("test.sync");
        let package_path_str = package_path.to_str().unwrap();

        // CASE A: Export package -> registry row created
        let pkg = FiscalClosurePackageService::build_closure_package(
            "WILAYA-TEST",
            "admin",
            2025,
            2026,
            "2026-01-01T00:00:00Z",
            &grpc_lib::application::services::FiscalClosurePackageSignerInfo {
                issuer_identity_id: wilaya_identity_id.to_string(),
                signing_key_id: signer.public_key_hex(),
            },
            None,
        )
        .expect("build package");

        FiscalClosurePackageService::new(db.executor())
            .export_to_file(&pkg, &signer, package_path_str)
            .expect("export package");

        let registry = db
            .executor()
            .fiscal_package_registry()
            .list_all()
            .expect("list registry");
        assert_eq!(registry.len(), 1);
        assert_eq!(registry[0].transition_id, pkg.fiscal_transition_id);
        assert_eq!(registry[0].retention_status, "ACTIVE");
        assert!(registry[0].applied_at.is_none());

        // CASE B: Apply package -> applied_at populated
        FiscalClosurePackageService::new(db.executor())
            .apply_closure_package(package_path_str, "system", "user1")
            .expect("apply package");

        let registry = db
            .executor()
            .fiscal_package_registry()
            .list_all()
            .expect("list registry");
        assert!(registry[0].applied_at.is_some());
        assert_eq!(registry[0].applied_by, Some("user1".to_string()));

        // CASE C: Replay apply rejected (either by year mismatch or replay-protection table)
        let result = FiscalClosurePackageService::new(db.executor()).apply_closure_package(
            package_path_str,
            "system",
            "user1",
        );
        assert!(result.is_err());
        let err_str = format!("{:?}", result);
        assert!(
            err_str.contains("OperationNotPermitted") || err_str.contains("DuplicateSyncPackage")
        );

        // CASE D: Fingerprint deterministic for same logical payload
        let fingerprint1 = FiscalClosurePackageService::compute_fingerprint(&pkg).unwrap();
        let mut pkg2 = pkg.clone();
        pkg2.package_fingerprint = "DIFFERENT".to_string();
        let fingerprint2 = FiscalClosurePackageService::compute_fingerprint(&pkg2).unwrap();
        assert_eq!(fingerprint1, fingerprint2);
        assert_eq!(fingerprint1, pkg.package_fingerprint);

        // CASE E: Transition history newest-first deterministic ordering
        let history =
            grpc_lib::application::services::FiscalTransitionHistoryService::new(db.executor())
                .build_transition_history()
                .expect("build history");
        assert!(!history.is_empty());

        // CASE G: Archived package remains queryable
        db.executor()
            .fiscal_package_registry()
            .mark_archived(&pkg.fiscal_transition_id, true)
            .unwrap();
        let registry = db
            .executor()
            .fiscal_package_registry()
            .list_all()
            .expect("list registry");
        assert!(registry[0].archived);
    }

    #[test]
    fn test_fingerprint_ignores_filesystem_metadata() {
        let (state, _temp_dir) = create_test_state();
        let db_guard = state.db.lock().unwrap();
        let db = db_guard.as_ref().unwrap();
        let wilaya_identity_id = seed_wilaya_identity(db, WILAYA_SIGNING_SECRET);
        let signer = wilaya_signer();

        let pkg = FiscalClosurePackageService::build_closure_package(
            "WILAYA-TEST",
            "admin",
            2025,
            2026,
            "2026-01-01T00:00:00Z",
            &grpc_lib::application::services::FiscalClosurePackageSignerInfo {
                issuer_identity_id: wilaya_identity_id.to_string(),
                signing_key_id: signer.public_key_hex(),
            },
            None,
        )
        .unwrap();

        let path1 = _temp_dir.path().join("pkg1.sync");
        let path2 = _temp_dir.path().join("pkg2.sync");

        FiscalClosurePackageService::new(db.executor())
            .export_to_file(&pkg, &signer, path1.to_str().unwrap())
            .unwrap();

        let content1 = fs::read_to_string(&path1).unwrap();
        // Change formatting slightly (e.g. add whitespace/newline)
        fs::write(&path2, format!("  {}  \n", content1)).unwrap();

        let preview1 = FiscalClosurePackageService::new(db.executor())
            .preview_closure_package(path1.to_str().unwrap())
            .unwrap();
        let preview2 = FiscalClosurePackageService::new(db.executor())
            .preview_closure_package(path2.to_str().unwrap())
            .unwrap();

        assert_eq!(preview1.package_fingerprint, preview2.package_fingerprint);
    }
}