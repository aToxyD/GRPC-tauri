//! Licensing enforcement gate (ADR-0042 §5).
//!
//! Behavioral contract:
//! - The gate is DORMANT until an active trust anchor is installed. Installing
//!   an anchor makes the gate globally ACTIVE for non-exempt actions.
//! - Exempt actions (system config, audit observability, licensing management,
//!   identity provisioning) always pass the gate.
//! - When active, a non-exempt action succeeds ONLY if at least one stored
//!   license (a) is `active`, (b) re-verifies against the active anchor via the
//!   FULL pipeline, (c) is bound to this node, and (d) declares the required
//!   entitlement. Any failure ⇒ deny (fail-closed).
//! - Error precedence: license missing ⇒ `LicenseRequired`; bound license
//!   present but lacking the entitlement ⇒ `EntitlementRequired`; licenses
//!   present but bound to a different node ⇒ `LicenseNotForThisNode`.
//!
//! Database unavailability is treated as deny (fail-closed) — the gate never
//! fails open.

use crate::application::authz::{Action, AuthorizationError};
use crate::application::licensing::subject::{evaluate_subject, SubjectBinding};
use crate::db::Database;
use crate::infrastructure::licensing::verification::{verify, AnchorRef};
use crate::models::{EntitlementKey, LicenseStatus, SignedLicenseArtifact};
use crate::repositories::licensing_anchor::LicensingAnchorRepository;
use crate::repositories::licensing_license::{LicensingLicenseRepository, LicensingLicenseRow};

/// Maps an `Action` to the entitlement that must be held for it to proceed.
/// `None` = exempt (always passes the gate). Contract v1 mapping table
/// (artifact-spec §5): `core.auth` / `core.sync` / `core.reports` /
/// `core.stock` / `core.consume` / `core.admin`.
pub fn entitlement_for_action(action: Action) -> Option<EntitlementKey> {
    use EntitlementKey as E;
    match action {
        // Stock domain (products / inventory / orders / movements).
        Action::ManageProducts
        | Action::ReadProducts
        | Action::ManageInventory
        | Action::ReadInventory
        | Action::ManageOrders
        | Action::ReadOrders
        | Action::ExportProducts
        | Action::ImportProductsPackage
        | Action::ExportStockMovements
        | Action::ExportStockMovementsPackage
        | Action::ImportStockMovementsPackage
        | Action::ImportStockMovements => Some(E::CoreStock),

        // Reports domain.
        Action::ReadDailyReports
        | Action::ManageDailyReports
        | Action::ReadMonthlySummary
        | Action::ReadWilayaReports
        | Action::ExportDailyReport
        | Action::ExportMonthlySummary
        | Action::ImportDailyReportPackage
        | Action::ImportMonthlySummaryPackage
        | Action::ReadImportAudit => Some(E::CoreReports),

        // Sync domain (identity/registry distribution, conflict management).
        Action::ManageSyncConflicts
        | Action::ImportTrustPackage
        | Action::ImportRegistryPackage
        | Action::ManageAccountSync
        | Action::ExportIdentityAccessPackage
        | Action::ImportIdentityAccessPackage => Some(E::CoreSync),

        // Admin domain (unit management, fiscal lifecycle).
        Action::ManageUnits
        | Action::ReadUnits
        | Action::CloseFiscalYearAuthority
        | Action::ApplyFiscalTransition => Some(E::CoreAdmin),

        // Identity domain (credential lifecycle → core.auth).
        Action::RotateCredential | Action::ReissueCredential | Action::ReadUserActivity => {
            Some(E::CoreAuth)
        }

        // Exempt: licensing management, system configuration, audit/health
        // observability, generic policy helpers. These remain available so
        // operators can provision, diagnose, and audit the gate itself.
        Action::ReadLicensingStatus
        | Action::ManageLicensing
        | Action::ConfigureAsWilaya
        | Action::ReadSystemMetrics
        | Action::ViewSystemHealth
        | Action::ReadAuditLog
        | Action::ManageBackups
        | Action::AdminOnly
        | Action::AuthenticatedOnly => None,
    }
}

/// Enforces the licensing gate for `action`. Denies (fail-closed) on any
/// database or verification error.
pub fn enforce(
    db: &Database,
    node_public_key: Option<Vec<u8>>,
    action: Action,
) -> Result<(), AuthorizationError> {
    let Some(required) = entitlement_for_action(action) else {
        return Ok(());
    };
    // Persistence-derived dormant/active trigger (ADR-0042 §5).
    let anchor = match LicensingAnchorRepository::new(db.executor()).get_active() {
        Ok(Some(anchor)) => anchor,
        Ok(None) => return Ok(()),
        Err(_) => return Err(AuthorizationError::LicenseRequired),
    };
    // Fail-closed: without this node's key, binding cannot be established.
    let Some(node_key) = node_public_key.filter(|key| key.len() == 32) else {
        return Err(AuthorizationError::LicenseRequired);
    };
    let licenses = match LicensingLicenseRepository::new(db.executor()).list_all() {
        Ok(licenses) => licenses,
        Err(_) => return Err(AuthorizationError::LicenseRequired),
    };
    let mut bound_license_seen = false;
    let mut other_node_license_seen = false;
    for license in &licenses {
        if !declares_entitlement(license, &anchor, &node_key, required) {
            continue;
        }
        return Ok(());
    }
    // No license grants the entitlement. Distinguish the denial reasons.
    for license in &licenses {
        if !is_verifiably_valid(license, &anchor) {
            continue;
        }
        match evaluate_subject(&license.subject_id, Some(&node_key)) {
            SubjectBinding::Bound => bound_license_seen = true,
            SubjectBinding::Mismatch => other_node_license_seen = true,
            _ => {}
        }
    }
    if bound_license_seen {
        Err(AuthorizationError::EntitlementRequired {
            entitlement: required.key().to_string(),
        })
    } else if other_node_license_seen {
        Err(AuthorizationError::LicenseNotForThisNode)
    } else {
        Err(AuthorizationError::LicenseRequired)
    }
}

/// True only when the license is Active, re-verifies against the active anchor,
/// is bound to this node, and declares the required entitlement.
fn declares_entitlement(
    license: &LicensingLicenseRow,
    anchor: &crate::repositories::licensing_anchor::LicensingAnchorRow,
    node_key: &[u8],
    required: EntitlementKey,
) -> bool {
    if !is_verifiably_valid(license, anchor) {
        return false;
    }
    if !matches!(
        evaluate_subject(&license.subject_id, Some(node_key)),
        SubjectBinding::Bound
    ) {
        return false;
    }
    let Ok(parsed) = serde_json::from_str::<SignedLicenseArtifact>(&license.artifact_json) else {
        return false;
    };
    parsed
        .payload
        .entitlements
        .iter()
        .any(|key| EntitlementKey::from_key(key) == Some(required))
}

/// Declared Active state + FULL verification pipeline against the anchor.
fn is_verifiably_valid(
    license: &LicensingLicenseRow,
    anchor: &crate::repositories::licensing_anchor::LicensingAnchorRow,
) -> bool {
    if !LicenseStatus::from_key(&license.status)
        .map(|status| status.is_enforceable())
        .unwrap_or(false)
    {
        return false;
    }
    verify(
        license.artifact_json.as_bytes(),
        &AnchorRef {
            key_id: &anchor.key_id,
            public_key: &anchor.public_key,
        },
    )
    .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::licensing::test_support::{build_artifact, Licensor};
    use crate::application::licensing::trust_anchor_service::TrustAnchorService;
    use crate::db::ConnectionFactory;
    use crate::infrastructure::licensing::encoding::encode_base64url;
    use crate::repositories::licensing_license::LicensingLicenseRepository;

    const NODE_SECRET: [u8; 32] = [42u8; 32];
    const OTHER_NODE_SECRET: [u8; 32] = [7u8; 32];

    fn node_public_key() -> Vec<u8> {
        Licensor::node_public_key(&NODE_SECRET)
    }

    fn other_node_public_key() -> Vec<u8> {
        Licensor::node_public_key(&OTHER_NODE_SECRET)
    }

    fn subject_of(public_key: &[u8]) -> String {
        encode_base64url(public_key)
    }

    /// Installs `licensor`'s anchor and imports an artifact signed by the SAME
    /// licensor (subject + entitlements + status). Returns the db.
    fn db_with_license(
        licensor: &Licensor,
        subject: &str,
        entitlements: &[&str],
        status: &str,
        import_node_key: Option<Vec<u8>>,
    ) -> crate::db::Database {
        let mut db = ConnectionFactory::new_for_test().unwrap();
        TrustAnchorService::import_anchor(&mut db, &licensor.provisioning_package()).unwrap();
        let artifact = build_artifact(licensor, subject, entitlements, status);
        crate::application::licensing::LicenseVerificationService::import_license(
            &mut db,
            import_node_key,
            &artifact,
        )
        .unwrap();
        db
    }

    fn node_key() -> Vec<u8> {
        node_public_key()
    }

    #[test]
    fn exempt_actions_always_pass() {
        let db = ConnectionFactory::new_for_test().unwrap();
        for action in [
            Action::ReadLicensingStatus,
            Action::ManageLicensing,
            Action::ReadAuditLog,
            Action::ViewSystemHealth,
            Action::ConfigureAsWilaya,
            Action::ManageBackups,
        ] {
            assert!(
                enforce(&db, Some(node_key()), action).is_ok(),
                "exempt action {:?} must pass even on a fresh db",
                action
            );
        }
    }

    #[test]
    fn gate_is_dormant_without_anchor() {
        let db = ConnectionFactory::new_for_test().unwrap();
        // Non-exempt stock action, but no anchor ⇒ dormant ⇒ allowed.
        assert!(enforce(&db, Some(node_key()), Action::ManageOrders).is_ok());
        assert!(enforce(&db, None, Action::ManageOrders).is_ok());
    }

    #[test]
    fn gate_denies_without_any_license() {
        let mut db = ConnectionFactory::new_for_test().unwrap();
        let licensor = Licensor::new("lk-test-0001");
        TrustAnchorService::import_anchor(&mut db, &licensor.provisioning_package()).unwrap();
        let err = enforce(&db, Some(node_key()), Action::ManageOrders).unwrap_err();
        assert!(matches!(err, AuthorizationError::LicenseRequired));
        // Missing node key also fails closed (can't establish binding).
        let err = enforce(&db, None, Action::ManageOrders).unwrap_err();
        assert!(matches!(err, AuthorizationError::LicenseRequired));
    }

    #[test]
    fn gate_grants_when_bound_license_has_entitlement() {
        let licensor = Licensor::new("lk-test-0001");
        let db = db_with_license(
            &licensor,
            &subject_of(&node_public_key()),
            &["core.stock"],
            "active",
            Some(node_key()),
        );
        assert!(enforce(&db, Some(node_key()), Action::ManageOrders).is_ok());
        assert!(enforce(&db, Some(node_key()), Action::ManageProducts).is_ok());
    }

    #[test]
    fn gate_denies_when_bound_license_lacks_entitlement() {
        let licensor = Licensor::new("lk-test-0001");
        let db = db_with_license(
            &licensor,
            &subject_of(&node_public_key()),
            &["core.reports"],
            "active",
            Some(node_key()),
        );
        let err = enforce(&db, Some(node_key()), Action::ManageOrders).unwrap_err();
        match err {
            AuthorizationError::EntitlementRequired { entitlement } => {
                assert_eq!(entitlement, "core.stock")
            }
            other => panic!("expected EntitlementRequired, got {other:?}"),
        }
        // The same license DOES grant reports.
        assert!(enforce(&db, Some(node_key()), Action::ManageDailyReports).is_ok());
    }

    #[test]
    fn gate_denies_when_only_license_belongs_to_other_node() {
        let licensor = Licensor::new("lk-test-0001");
        let db = db_with_license(
            &licensor,
            &subject_of(&other_node_public_key()),
            &["core.stock"],
            "active",
            Some(other_node_public_key()),
        );
        let err = enforce(&db, Some(node_key()), Action::ManageOrders).unwrap_err();
        assert!(matches!(err, AuthorizationError::LicenseNotForThisNode));
    }

    #[test]
    fn gate_denies_when_bound_license_is_revoked_or_expired() {
        for status in ["revoked", "expired", "suspended"] {
            let licensor = Licensor::new("lk-test-0001");
            let db = db_with_license(
                &licensor,
                &subject_of(&node_public_key()),
                &["core.stock"],
                status,
                Some(node_key()),
            );
            let err = enforce(&db, Some(node_key()), Action::ManageOrders).unwrap_err();
            assert!(
                matches!(err, AuthorizationError::LicenseRequired),
                "status {status} must fail closed, got {err:?}"
            );
        }
    }

    #[test]
    fn gate_denies_when_license_signed_by_retired_anchor() {
        // Install anchor A, import a license, then rotate to anchor B: the
        // stored license no longer verifies ⇒ deny.
        let mut db = ConnectionFactory::new_for_test().unwrap();
        let a = Licensor::new("lk-a");
        TrustAnchorService::import_anchor(&mut db, &a.provisioning_package()).unwrap();
        let subject = subject_of(&node_public_key());
        let artifact = build_artifact(&a, &subject, &["core.stock"], "active");
        crate::application::licensing::LicenseVerificationService::import_license(
            &mut db,
            Some(node_key()),
            &artifact,
        )
        .unwrap();
        let b = Licensor::new("lk-b");
        TrustAnchorService::import_anchor(&mut db, &b.provisioning_package()).unwrap();
        let err = enforce(&db, Some(node_key()), Action::ManageOrders).unwrap_err();
        assert!(matches!(err, AuthorizationError::LicenseRequired));
        // The status projection reflects non-enforceability.
        let status = crate::application::licensing::LicenseVerificationService::status(
            &db,
            Some(node_key()),
        )
        .unwrap();
        assert!(!status.licenses[0].enforceable);
    }

    #[test]
    fn license_row_storage_supports_reverification_after_rotation() {
        // Defensive: re-import under the new anchor restores enforcement.
        let mut db = ConnectionFactory::new_for_test().unwrap();
        let a = Licensor::new("lk-a");
        TrustAnchorService::import_anchor(&mut db, &a.provisioning_package()).unwrap();
        let subject = subject_of(&node_public_key());
        let artifact = build_artifact(&a, &subject, &["core.stock"], "active");
        crate::application::licensing::LicenseVerificationService::import_license(
            &mut db,
            Some(node_key()),
            &artifact,
        )
        .unwrap();
        let b = Licensor::new("lk-b");
        TrustAnchorService::import_anchor(&mut db, &b.provisioning_package()).unwrap();
        assert!(enforce(&db, Some(node_key()), Action::ManageOrders).is_err());
        let new_artifact = build_artifact(&b, &subject, &["core.stock"], "active");
        crate::application::licensing::LicenseVerificationService::import_license(
            &mut db,
            Some(node_key()),
            &new_artifact,
        )
        .unwrap();
        assert!(enforce(&db, Some(node_key()), Action::ManageOrders).is_ok());
        assert_eq!(
            LicensingLicenseRepository::new(db.executor())
                .count()
                .unwrap(),
            1
        );
    }
}
