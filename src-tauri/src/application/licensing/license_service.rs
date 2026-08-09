//! License import / verification service (ADR-0042 §3–§7).
//!
//! Runs the FULL verification pipeline against the active trust anchor on
//! every import: structure, version, identity, canonicality, key binding,
//! signature (artifact-spec §4 steps 1–6), then the semantic rules the
//! infrastructure layer does not own (type registry, status, entitlements,
//! subject binding). Any failure at any step rejects the artifact (fail-closed).
//!
//! Import REPLACES the derived view for a license id (full overwrite, never a
//! merge — ADR-0003 Invariant 2). The exact canonical bytes are persisted so
//! enforcement and `verify_license` can re-run the FULL pipeline deterministically.

use chrono::Utc;

use crate::application::licensing::subject::{evaluate_subject, SubjectBinding};
use crate::application::licensing::trust_anchor_service::TrustAnchorService;
use crate::db::Database;
use crate::errors::AppResult;
use crate::infrastructure::licensing::verification::{verify, AnchorRef};
use crate::models::{
    EntitlementKey, ImportLicenseResult, LicenseStatus, LicenseView, LicensingStatus,
    SignedLicenseArtifact, VerifyLicenseReport,
};
use crate::repositories::licensing_anchor::LicensingAnchorRow;
use crate::repositories::licensing_events::{LicensingEventRow, LicensingEventsRepository};
use crate::repositories::licensing_license::{LicensingLicenseRepository, LicensingLicenseRow};

pub struct LicenseVerificationService;

impl LicenseVerificationService {
    /// Imports and enforces a signed license artifact (ADR-0042 §3/§7).
    ///
    /// Never fails the command on a verifiable-but-invalid artifact: a failed
    /// verification, unknown semantic content, or a license for a different
    /// node are returned as structured outcomes, not errors.
    pub fn import_license(
        db: &mut Database,
        node_public_key: Option<Vec<u8>>,
        artifact_json: &str,
    ) -> AppResult<ImportLicenseResult> {
        let Some(anchor) = TrustAnchorService::active(db)? else {
            let message =
                "no active licensing trust anchor installed — refusing import".to_string();
            record_event(db, "import", "rejected", None, None, None, Some(&message))?;
            return Ok(rejected(None, None, message));
        };
        let artifact = match verify(
            artifact_json.as_bytes(),
            &AnchorRef {
                key_id: &anchor.key_id,
                public_key: &anchor.public_key,
            },
        ) {
            Ok(artifact) => artifact,
            Err(e) => {
                let message = e.to_string();
                record_event(
                    db,
                    "import",
                    "rejected",
                    None,
                    None,
                    Some(&anchor.key_id),
                    Some(&message),
                )?;
                return Ok(rejected(None, None, message));
            }
        };
        if let Err(message) = validate_semantics(&artifact) {
            record_event(
                db,
                "import",
                "rejected",
                None,
                None,
                Some(&anchor.key_id),
                Some(&message),
            )?;
            return Ok(rejected(None, Some(artifact.metadata.artifact_id), message));
        }
        let license_id = artifact.payload.license.id.clone();
        let artifact_id = artifact.metadata.artifact_id.clone();
        match evaluate_subject(&artifact.payload.subject.id, node_public_key.as_deref()) {
            SubjectBinding::NodeKeyMissing => {
                let message =
                    "node key not provisioned — subject binding cannot be evaluated".to_string();
                record_event(
                    db,
                    "import",
                    "rejected",
                    None,
                    None,
                    Some(&anchor.key_id),
                    Some(&message),
                )?;
                return Ok(rejected(Some(license_id), Some(artifact_id), message));
            }
            SubjectBinding::InvalidSubject => {
                let message =
                    "invalid subject.id encoding (must be base64url of a 32-byte public key)"
                        .to_string();
                record_event(
                    db,
                    "import",
                    "rejected",
                    None,
                    None,
                    Some(&anchor.key_id),
                    Some(&message),
                )?;
                return Ok(rejected(Some(license_id), Some(artifact_id), message));
            }
            SubjectBinding::Mismatch => {
                // Valid signature, bound to a different node: NOT a rejection
                // (ADR-0042 §4 distinction).
                let message = "license is not bound to this node".to_string();
                record_event(
                    db,
                    "import",
                    "not-for-this-node",
                    None,
                    None,
                    Some(&anchor.key_id),
                    Some(&message),
                )?;
                return Ok(ImportLicenseResult {
                    outcome: "not-for-this-node".to_string(),
                    license_id: Some(license_id),
                    artifact_id: Some(artifact_id),
                    message,
                });
            }
            SubjectBinding::Bound => {}
        }
        let now = Utc::now().to_rfc3339();
        let row = LicensingLicenseRow {
            license_id: license_id.clone(),
            artifact_id: artifact_id.clone(),
            type_key: artifact.payload.license.type_key.clone(),
            subject_id: artifact.payload.subject.id.clone(),
            entitlements_json: serde_json::to_string(&artifact.payload.entitlements)
                .unwrap_or_else(|_| "[]".to_string()),
            status: artifact.payload.license.status.clone(),
            contract_version_major: artifact.payload.contract_version.major,
            contract_version_minor: artifact.payload.contract_version.minor,
            artifact_json: artifact_json.to_string(),
            imported_at: now.clone(),
            last_verified_at: Some(now.clone()),
        };
        db.with_transaction(|tx| {
            LicensingLicenseRepository::new(tx).upsert(&row)?;
            LicensingEventsRepository::new(tx).record(&LicensingEventRow::new(
                "import",
                "verified",
                Some(&license_id),
                Some(&artifact_id),
                Some(&anchor.key_id),
                None,
            ))?;
            Ok(())
        })?;
        Ok(ImportLicenseResult {
            outcome: "imported".to_string(),
            license_id: Some(license_id),
            artifact_id: Some(artifact_id),
            message: "license imported".to_string(),
        })
    }

    /// Deterministic pre-import check: runs the FULL pipeline without
    /// persisting anything (ADR-0042 §3). Same outcomes as import.
    pub fn dry_run_verify(
        db: &Database,
        node_public_key: Option<Vec<u8>>,
        artifact_json: &str,
    ) -> AppResult<ImportLicenseResult> {
        let Some(anchor) = TrustAnchorService::active(db)? else {
            return Ok(rejected(
                None,
                None,
                "no active licensing trust anchor installed".to_string(),
            ));
        };
        let artifact = match verify(
            artifact_json.as_bytes(),
            &AnchorRef {
                key_id: &anchor.key_id,
                public_key: &anchor.public_key,
            },
        ) {
            Ok(artifact) => artifact,
            Err(e) => return Ok(rejected(None, None, e.to_string())),
        };
        if let Err(message) = validate_semantics(&artifact) {
            return Ok(rejected(None, Some(artifact.metadata.artifact_id), message));
        }
        let license_id = artifact.payload.license.id.clone();
        let artifact_id = artifact.metadata.artifact_id.clone();
        match evaluate_subject(&artifact.payload.subject.id, node_public_key.as_deref()) {
            SubjectBinding::NodeKeyMissing => Ok(rejected(
                Some(license_id),
                Some(artifact_id),
                "node key not provisioned — subject binding cannot be evaluated".to_string(),
            )),
            SubjectBinding::InvalidSubject => Ok(rejected(
                Some(license_id),
                Some(artifact_id),
                "invalid subject.id encoding (must be base64url of a 32-byte public key)"
                    .to_string(),
            )),
            SubjectBinding::Mismatch => Ok(ImportLicenseResult {
                outcome: "not-for-this-node".to_string(),
                license_id: Some(license_id),
                artifact_id: Some(artifact_id),
                message: "license is not bound to this node".to_string(),
            }),
            SubjectBinding::Bound => Ok(ImportLicenseResult {
                outcome: "imported".to_string(),
                license_id: Some(license_id),
                artifact_id: Some(artifact_id),
                message: "license valid".to_string(),
            }),
        }
    }

    /// Deterministic re-verification of every stored license (ADR-0042 §3):
    /// re-runs the FULL pipeline from the persisted canonical bytes and
    /// refreshes `last_verified_at` for licenses that remain enforceable.
    pub fn reverify_stored(
        db: &Database,
        node_public_key: Option<Vec<u8>>,
    ) -> AppResult<VerifyLicenseReport> {
        let Some(anchor) = TrustAnchorService::active(db)? else {
            return Ok(VerifyLicenseReport {
                checked: 0,
                enforceable: 0,
                results: Vec::new(),
            });
        };
        let rows = LicensingLicenseRepository::new(db.executor()).list_all()?;
        let mut results = Vec::with_capacity(rows.len());
        let mut enforceable_count = 0u64;
        let now = Utc::now().to_rfc3339();
        for row in rows {
            let enforceable = evaluate_enforceable(&row, &anchor, node_public_key.as_deref());
            if enforceable {
                enforceable_count += 1;
                LicensingLicenseRepository::new(db.executor())
                    .update_last_verified(&row.license_id, &now)?;
            }
            results.push(license_view(&row, enforceable));
        }
        Ok(VerifyLicenseReport {
            checked: results.len() as u64,
            enforceable: enforceable_count,
            results,
        })
    }

    /// Live licensing projection for `get_licensing_status`.
    pub fn status(db: &Database, node_public_key: Option<Vec<u8>>) -> AppResult<LicensingStatus> {
        let active_anchor = TrustAnchorService::active(db)?;
        let anchor_view = TrustAnchorService::view(db)?;
        let anchor_installed = anchor_view.installed;
        let rows = LicensingLicenseRepository::new(db.executor()).list_all()?;
        let mut views = Vec::with_capacity(rows.len());
        let mut active_license_count = 0u64;
        for row in rows {
            if LicenseStatus::from_key(&row.status)
                .map(|status| status.is_enforceable())
                .unwrap_or(false)
            {
                active_license_count += 1;
            }
            let enforceable = match &active_anchor {
                Some(anchor) => evaluate_enforceable(&row, anchor, node_public_key.as_deref()),
                None => false,
            };
            views.push(license_view(&row, enforceable));
        }
        let license_count = views.len() as u64;
        Ok(LicensingStatus {
            anchor: anchor_view,
            licenses: views,
            summary: crate::models::LicensingSummary {
                anchor_installed,
                license_count,
                active_license_count,
                gate_active: anchor_installed,
            },
        })
    }
}

/// Semantic validation the infrastructure verifier deliberately does not own
/// (ADR-0042 §3 / artifact-spec §5): type registry, declared status,
/// entitlement registry, `issued_for` ≡ `subject.id` consistency.
fn validate_semantics(artifact: &SignedLicenseArtifact) -> Result<(), String> {
    if artifact.payload.license.id.trim().is_empty() {
        return Err("license id is empty".to_string());
    }
    if artifact.payload.license.type_key != "production" {
        return Err(format!(
            "unsupported license type '{}'",
            artifact.payload.license.type_key
        ));
    }
    if LicenseStatus::from_key(&artifact.payload.license.status).is_none() {
        return Err(format!(
            "unknown license status '{}'",
            artifact.payload.license.status
        ));
    }
    for entitlement in &artifact.payload.entitlements {
        if EntitlementKey::from_key(entitlement).is_none() {
            return Err(format!("unknown entitlement '{entitlement}'"));
        }
    }
    if artifact.metadata.issued_for != artifact.payload.subject.id {
        return Err("metadata.issued_for must equal payload.subject.id".to_string());
    }
    Ok(())
}

/// Whether a stored license currently enforces (ADR-0042 §3 re-verification):
/// declared Active state + FULL pipeline against the active anchor + subject
/// bound to this node. All three must hold; anything else ⇒ `false` (fail-closed).
fn evaluate_enforceable(
    row: &LicensingLicenseRow,
    anchor: &LicensingAnchorRow,
    node_public_key: Option<&[u8]>,
) -> bool {
    if !LicenseStatus::from_key(&row.status)
        .map(|status| status.is_enforceable())
        .unwrap_or(false)
    {
        return false;
    }
    if verify(
        row.artifact_json.as_bytes(),
        &AnchorRef {
            key_id: &anchor.key_id,
            public_key: &anchor.public_key,
        },
    )
    .is_err()
    {
        return false;
    }
    matches!(
        evaluate_subject(&row.subject_id, node_public_key),
        SubjectBinding::Bound
    )
}

fn license_view(row: &LicensingLicenseRow, enforceable: bool) -> LicenseView {
    LicenseView {
        license_id: row.license_id.clone(),
        artifact_id: row.artifact_id.clone(),
        type_key: row.type_key.clone(),
        subject_id: row.subject_id.clone(),
        entitlements: serde_json::from_str(&row.entitlements_json).unwrap_or_default(),
        status: row.status.clone(),
        contract_version_major: row.contract_version_major,
        contract_version_minor: row.contract_version_minor,
        imported_at: row.imported_at.clone(),
        last_verified_at: row.last_verified_at.clone(),
        enforceable,
    }
}

fn rejected(
    license_id: Option<String>,
    artifact_id: Option<String>,
    message: String,
) -> ImportLicenseResult {
    ImportLicenseResult {
        outcome: "rejected".to_string(),
        license_id,
        artifact_id,
        message,
    }
}

fn record_event(
    db: &Database,
    event_type: &str,
    outcome: &str,
    license_id: Option<&str>,
    artifact_id: Option<&str>,
    key_id: Option<&str>,
    message: Option<&str>,
) -> AppResult<()> {
    LicensingEventsRepository::new(db.executor())
        .record(&LicensingEventRow::new(
            event_type,
            outcome,
            license_id,
            artifact_id,
            key_id,
            message,
        ))
        .map(|_| ())
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

    /// Installs an anchor and returns the licensor key it signs with.
    fn setup(db: &mut Database) -> Licensor {
        let licensor = Licensor::new("lk-test-0001");
        TrustAnchorService::import_anchor(db, &licensor.provisioning_package()).unwrap();
        licensor
    }

    fn subject_of(public_key: &[u8]) -> String {
        encode_base64url(public_key)
    }

    #[test]
    fn import_rejects_when_no_anchor_installed() {
        let mut db = ConnectionFactory::new_for_test().unwrap();
        let licensor = Licensor::new("lk-test-0001");
        let artifact = build_artifact(
            &licensor,
            &subject_of(&node_public_key()),
            &["core.stock"],
            "active",
        );
        let result =
            LicenseVerificationService::import_license(&mut db, Some(node_public_key()), &artifact)
                .unwrap();
        assert_eq!(result.outcome, "rejected");
        assert!(result.message.contains("no active licensing trust anchor"));
        assert!(
            LicensingLicenseRepository::new(db.executor())
                .count()
                .unwrap()
                == 0
        );
    }

    #[test]
    fn import_rejects_signature_from_foreign_key() {
        // Same key_id but signed with a different key ⇒ InvalidSignature.
        let mut db = ConnectionFactory::new_for_test().unwrap();
        let anchor_licensor = Licensor::new("lk-shared");
        TrustAnchorService::import_anchor(&mut db, &anchor_licensor.provisioning_package())
            .unwrap();
        let foreign = Licensor::new("lk-shared");
        let artifact = build_artifact(
            &foreign,
            &subject_of(&node_public_key()),
            &["core.stock"],
            "active",
        );
        let result =
            LicenseVerificationService::import_license(&mut db, Some(node_public_key()), &artifact)
                .unwrap();
        assert_eq!(result.outcome, "rejected");
        assert!(
            result.message.contains("signature"),
            "got {}",
            result.message
        );
    }

    #[test]
    fn import_rejects_unknown_entitlement() {
        let mut db = ConnectionFactory::new_for_test().unwrap();
        let licensor = setup(&mut db);
        let artifact = build_artifact(
            &licensor,
            &subject_of(&node_public_key()),
            &["core.unknown"],
            "active",
        );
        let result =
            LicenseVerificationService::import_license(&mut db, Some(node_public_key()), &artifact)
                .unwrap();
        assert_eq!(result.outcome, "rejected");
        assert!(
            result.message.contains("unknown entitlement"),
            "got {}",
            result.message
        );
    }

    #[test]
    fn import_rejects_unknown_status() {
        let mut db = ConnectionFactory::new_for_test().unwrap();
        let licensor = setup(&mut db);
        let artifact = build_artifact(
            &licensor,
            &subject_of(&node_public_key()),
            &["core.stock"],
            "bogus",
        );
        let result =
            LicenseVerificationService::import_license(&mut db, Some(node_public_key()), &artifact)
                .unwrap();
        assert_eq!(result.outcome, "rejected");
        assert!(
            result.message.contains("unknown license status"),
            "got {}",
            result.message
        );
    }

    #[test]
    fn import_rejects_unknown_type() {
        let mut db = ConnectionFactory::new_for_test().unwrap();
        let licensor = setup(&mut db);
        let payload = crate::models::ArtifactPayload {
            license: crate::models::LicensePayload {
                id: "lic-x".into(),
                type_key: "evaluation".into(),
                status: "active".into(),
            },
            subject: crate::models::SubjectPayload {
                id: subject_of(&node_public_key()),
            },
            entitlements: vec!["core.stock".into()],
            contract_version: crate::models::ContractVersionPayload { major: 1, minor: 0 },
        };
        let artifact = build_with_payload(&licensor, payload);
        let result =
            LicenseVerificationService::import_license(&mut db, Some(node_public_key()), &artifact)
                .unwrap();
        assert_eq!(result.outcome, "rejected");
        assert!(
            result.message.contains("unsupported license type"),
            "got {}",
            result.message
        );
    }

    #[test]
    fn import_rejects_issued_for_subject_mismatch() {
        let mut db = ConnectionFactory::new_for_test().unwrap();
        let licensor = setup(&mut db);
        let subject = subject_of(&node_public_key());
        let payload = crate::models::ArtifactPayload {
            license: crate::models::LicensePayload {
                id: "lic-x".into(),
                type_key: "production".into(),
                status: "active".into(),
            },
            subject: crate::models::SubjectPayload { id: subject },
            entitlements: vec!["core.stock".into()],
            contract_version: crate::models::ContractVersionPayload { major: 1, minor: 0 },
        };
        let artifact = build_with_payload_and_issued_for(&licensor, payload, "different-subject");
        let result =
            LicenseVerificationService::import_license(&mut db, Some(node_public_key()), &artifact)
                .unwrap();
        assert_eq!(result.outcome, "rejected");
        assert!(
            result.message.contains("issued_for"),
            "got {}",
            result.message
        );
    }

    #[test]
    fn import_returns_not_for_this_node_for_other_node_subject() {
        let mut db = ConnectionFactory::new_for_test().unwrap();
        let licensor = setup(&mut db);
        let artifact = build_artifact(
            &licensor,
            &subject_of(&other_node_public_key()),
            &["core.stock"],
            "active",
        );
        let result =
            LicenseVerificationService::import_license(&mut db, Some(node_public_key()), &artifact)
                .unwrap();
        assert_eq!(result.outcome, "not-for-this-node");
        assert_eq!(result.license_id.as_deref(), Some("lic-test-0001"));
        assert!(
            LicensingLicenseRepository::new(db.executor())
                .count()
                .unwrap()
                == 0
        );
    }

    #[test]
    fn import_rejects_when_node_key_missing() {
        let mut db = ConnectionFactory::new_for_test().unwrap();
        let licensor = setup(&mut db);
        let artifact = build_artifact(
            &licensor,
            &subject_of(&node_public_key()),
            &["core.stock"],
            "active",
        );
        let result = LicenseVerificationService::import_license(&mut db, None, &artifact).unwrap();
        assert_eq!(result.outcome, "rejected");
        assert!(
            result.message.contains("binding cannot be evaluated"),
            "got {}",
            result.message
        );
    }

    #[test]
    fn import_rejects_invalid_subject_id_shape() {
        let mut db = ConnectionFactory::new_for_test().unwrap();
        let licensor = setup(&mut db);
        let artifact = build_artifact(
            &licensor,
            "not-a-valid-32-byte-key",
            &["core.stock"],
            "active",
        );
        let result =
            LicenseVerificationService::import_license(&mut db, Some(node_public_key()), &artifact)
                .unwrap();
        assert_eq!(result.outcome, "rejected");
        assert!(
            result.message.contains("subject.id"),
            "got {}",
            result.message
        );
    }

    #[test]
    fn import_success_persists_derived_view() {
        let mut db = ConnectionFactory::new_for_test().unwrap();
        let licensor = setup(&mut db);
        let artifact = build_artifact(
            &licensor,
            &subject_of(&node_public_key()),
            &["core.stock", "core.sync"],
            "active",
        );
        let result =
            LicenseVerificationService::import_license(&mut db, Some(node_public_key()), &artifact)
                .unwrap();
        assert_eq!(result.outcome, "imported");
        assert_eq!(result.license_id.as_deref(), Some("lic-test-0001"));
        let rows = LicensingLicenseRepository::new(db.executor())
            .list_all()
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].subject_id, subject_of(&node_public_key()));
        assert_eq!(rows[0].status, "active");
        assert!(rows[0].entitlements_json.contains("core.stock"));
        // The stored view is enforceable under the same node key.
        let report =
            LicenseVerificationService::reverify_stored(&db, Some(node_public_key())).unwrap();
        assert_eq!(report.checked, 1);
        assert_eq!(report.enforceable, 1);
    }

    #[test]
    fn import_overwrites_same_license_id() {
        let mut db = ConnectionFactory::new_for_test().unwrap();
        let licensor = setup(&mut db);
        let subject = subject_of(&node_public_key());
        let first = build_artifact(&licensor, &subject, &["core.stock"], "active");
        assert_eq!(
            LicenseVerificationService::import_license(&mut db, Some(node_public_key()), &first)
                .unwrap()
                .outcome,
            "imported"
        );
        let second = build_artifact(&licensor, &subject, &["core.sync"], "active");
        assert_eq!(
            LicenseVerificationService::import_license(&mut db, Some(node_public_key()), &second)
                .unwrap()
                .outcome,
            "imported"
        );
        let rows = LicensingLicenseRepository::new(db.executor())
            .list_all()
            .unwrap();
        assert_eq!(rows.len(), 1, "full overwrite, never a merge");
        assert!(rows[0].entitlements_json.contains("core.sync"));
        assert!(!rows[0].entitlements_json.contains("core.stock"));
    }

    #[test]
    fn revoked_license_imports_but_is_not_enforceable() {
        let mut db = ConnectionFactory::new_for_test().unwrap();
        let licensor = setup(&mut db);
        let artifact = build_artifact(
            &licensor,
            &subject_of(&node_public_key()),
            &["core.stock"],
            "revoked",
        );
        assert_eq!(
            LicenseVerificationService::import_license(&mut db, Some(node_public_key()), &artifact)
                .unwrap()
                .outcome,
            "imported"
        );
        let report =
            LicenseVerificationService::reverify_stored(&db, Some(node_public_key())).unwrap();
        assert_eq!(report.enforceable, 0);
        assert!(!report.results[0].enforceable);
        assert_eq!(report.results[0].status, "revoked");
    }

    #[test]
    fn dry_run_verify_does_not_persist() {
        let mut db = ConnectionFactory::new_for_test().unwrap();
        let licensor = setup(&mut db);
        let artifact = build_artifact(
            &licensor,
            &subject_of(&node_public_key()),
            &["core.stock"],
            "active",
        );
        let result =
            LicenseVerificationService::dry_run_verify(&db, Some(node_public_key()), &artifact)
                .unwrap();
        assert_eq!(result.outcome, "imported");
        assert!(
            LicensingLicenseRepository::new(db.executor())
                .count()
                .unwrap()
                == 0
        );
    }

    #[test]
    fn status_projects_anchor_licenses_and_gate() {
        let mut db = ConnectionFactory::new_for_test().unwrap();
        let licensor = setup(&mut db);
        let artifact = build_artifact(
            &licensor,
            &subject_of(&node_public_key()),
            &["core.stock"],
            "active",
        );
        LicenseVerificationService::import_license(&mut db, Some(node_public_key()), &artifact)
            .unwrap();
        let status = LicenseVerificationService::status(&db, Some(node_public_key())).unwrap();
        assert!(status.anchor.installed);
        assert_eq!(status.anchor.key_id.as_deref(), Some("lk-test-0001"));
        assert_eq!(status.licenses.len(), 1);
        assert!(status.licenses[0].enforceable);
        assert_eq!(status.summary.license_count, 1);
        assert_eq!(status.summary.active_license_count, 1);
        assert!(status.summary.gate_active);
    }

    #[test]
    fn status_without_anchor_is_dormant() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let status = LicenseVerificationService::status(&db, Some(node_public_key())).unwrap();
        assert!(!status.anchor.installed);
        assert!(status.licenses.is_empty());
        assert!(!status.summary.gate_active);
    }

    fn build_with_payload(licensor: &Licensor, payload: crate::models::ArtifactPayload) -> String {
        let mut artifact = crate::models::SignedLicenseArtifact {
            version: 1,
            metadata: crate::models::ArtifactMetadata {
                key_id: licensor.key_id.clone(),
                algorithm: "Ed25519".into(),
                artifact_id: String::new(),
                issued_for: payload.subject.id.clone(),
            },
            payload,
            signature: crate::models::ArtifactSignatureRef {
                algorithm: "Ed25519".into(),
                key_id: licensor.key_id.clone(),
                signature: String::new(),
            },
        };
        licensor.sign_artifact(&mut artifact)
    }

    fn build_with_payload_and_issued_for(
        licensor: &Licensor,
        payload: crate::models::ArtifactPayload,
        issued_for: &str,
    ) -> String {
        let mut artifact = crate::models::SignedLicenseArtifact {
            version: 1,
            metadata: crate::models::ArtifactMetadata {
                key_id: licensor.key_id.clone(),
                algorithm: "Ed25519".into(),
                artifact_id: String::new(),
                issued_for: issued_for.to_string(),
            },
            payload,
            signature: crate::models::ArtifactSignatureRef {
                algorithm: "Ed25519".into(),
                key_id: licensor.key_id.clone(),
                signature: String::new(),
            },
        };
        licensor.sign_artifact(&mut artifact)
    }
}
