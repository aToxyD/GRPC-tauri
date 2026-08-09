//! Trust anchor provisioning service (ADR-0042 §9).
//!
//! Installs / replaces the single active licensing Trust Anchor from a
//! `provisioning-v1` package (produced by the licensor's tooling). The anchor
//! is a public key: it grants NO operational trust, it merely (a) triggers the
//! enforcement gate and (b) authorizes licensing artifacts for the node.
//!
//! Rotation replaces the active anchor atomically (deactivate-all + insert
//! inside one transaction). Installing an anchor does NOT downgrade any
//! currently enforced license — re-verification (ADR-0042 §3) re-runs against
//! the new anchor on the next gate check and rejects artifacts signed by the
//! retired key.

use chrono::Utc;

use crate::db::Database;
use crate::errors::{AppError, AppResult, ValidationError};
use crate::infrastructure::licensing::encoding::decode_base64url;
use crate::infrastructure::licensing::provisioning::decode_provisioning_package;
use crate::infrastructure::licensing::signing::LICENSE_SIGNING_ALGORITHM;
use crate::models::{ImportAnchorResult, TrustAnchorView};
use crate::repositories::licensing_anchor::LicensingAnchorRepository;
use crate::repositories::licensing_events::LicensingEventRow;

pub struct TrustAnchorService;

impl TrustAnchorService {
    /// Validates and installs (or rotates) the single active anchor.
    pub fn import_anchor(db: &mut Database, package_json: &str) -> AppResult<ImportAnchorResult> {
        let package = decode_provisioning_package(package_json.as_bytes()).map_err(|e| {
            AppError::Validation(ValidationError::InvalidFormat {
                field: "package".into(),
                message: e.to_string(),
            })
        })?;
        // `decode_provisioning_package` guarantees 32 raw bytes; the repository
        // re-asserts the invariant defensively.
        let public_key = decode_base64url(&package.public_key).map_err(|e| {
            AppError::Validation(ValidationError::InvalidFormat {
                field: "public_key".into(),
                message: e.to_string(),
            })
        })?;
        if public_key.len() != 32 {
            return Err(AppError::Validation(ValidationError::InvalidFormat {
                field: "public_key".into(),
                message: "anchor public key must be exactly 32 bytes (Ed25519)".into(),
            }));
        }
        let now = Utc::now().to_rfc3339();
        let replaced_key_id = db.with_transaction(|tx| {
            let anchor_repo = LicensingAnchorRepository::new(tx);
            let prior = anchor_repo.get_active()?.map(|row| row.key_id);
            anchor_repo.deactivate_all()?;
            anchor_repo.insert_active(
                &package.key_id,
                &public_key,
                LICENSE_SIGNING_ALGORITHM,
                &now,
            )?;
            crate::repositories::licensing_events::LicensingEventsRepository::new(tx).record(
                &LicensingEventRow::new(
                    "anchor-install",
                    "installed",
                    None,
                    None,
                    Some(&package.key_id),
                    None,
                ),
            )?;
            Ok(prior)
        })?;
        Ok(ImportAnchorResult {
            installed: true,
            key_id: package.key_id,
            replaced_key_id,
        })
    }

    /// The active anchor, if installed.
    pub fn active(
        db: &Database,
    ) -> AppResult<Option<crate::repositories::licensing_anchor::LicensingAnchorRow>> {
        LicensingAnchorRepository::new(db.executor()).get_active()
    }

    /// Whether an active anchor exists (the enforcement-gate trigger).
    pub fn has_active(db: &Database) -> AppResult<bool> {
        LicensingAnchorRepository::new(db.executor()).has_active()
    }

    /// Live projection for `get_licensing_status`.
    pub fn view(db: &Database) -> AppResult<TrustAnchorView> {
        Ok(match Self::active(db)? {
            Some(anchor) => TrustAnchorView {
                installed: true,
                key_id: Some(anchor.key_id),
            },
            None => TrustAnchorView {
                installed: false,
                key_id: None,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::licensing::test_support::Licensor;
    use crate::db::ConnectionFactory;
    use crate::repositories::licensing_events::LicensingEventsRepository;

    #[test]
    fn import_anchor_then_active() {
        let mut db = ConnectionFactory::new_for_test().unwrap();
        let licensor = Licensor::new("lk-test-0001");
        let result =
            TrustAnchorService::import_anchor(&mut db, &licensor.provisioning_package()).unwrap();
        assert!(result.installed);
        assert_eq!(result.key_id, "lk-test-0001");
        assert_eq!(result.replaced_key_id, None);
        let active = TrustAnchorService::active(&db).unwrap().unwrap();
        assert_eq!(active.key_id, "lk-test-0001");
        assert_eq!(active.public_key, licensor.public_key());
        assert!(TrustAnchorService::has_active(&db).unwrap());
    }

    #[test]
    fn rotation_replaces_the_active_anchor() {
        let mut db = ConnectionFactory::new_for_test().unwrap();
        let old = Licensor::new("lk-old");
        let new = Licensor::new("lk-new");
        TrustAnchorService::import_anchor(&mut db, &old.provisioning_package()).unwrap();
        let result =
            TrustAnchorService::import_anchor(&mut db, &new.provisioning_package()).unwrap();
        assert_eq!(result.replaced_key_id.as_deref(), Some("lk-old"));
        let active = TrustAnchorService::active(&db).unwrap().unwrap();
        assert_eq!(active.key_id, "lk-new");
        assert_eq!(active.public_key, new.public_key());
    }

    #[test]
    fn no_anchor_by_default() {
        let db = ConnectionFactory::new_for_test().unwrap();
        assert!(TrustAnchorService::active(&db).unwrap().is_none());
        assert!(!TrustAnchorService::has_active(&db).unwrap());
        let view = TrustAnchorService::view(&db).unwrap();
        assert!(!view.installed);
        assert_eq!(view.key_id, None);
    }

    #[test]
    fn rejects_malformed_package_json() {
        let mut db = ConnectionFactory::new_for_test().unwrap();
        assert!(TrustAnchorService::import_anchor(&mut db, "{not-json").is_err());
        assert!(TrustAnchorService::import_anchor(&mut db, "{}").is_err());
        assert!(TrustAnchorService::import_anchor(&mut db, "null").is_err());
    }

    #[test]
    fn rejects_wrong_public_key_length() {
        let mut db = ConnectionFactory::new_for_test().unwrap();
        let value = serde_json::json!({
            "format": "provisioning-v1",
            "key_id": "lk-test",
            "algorithm": "Ed25519",
            "public_key": crate::infrastructure::licensing::encoding::encode_base64url(&[0u8; 16]),
        });
        let package = value.to_string();
        assert!(TrustAnchorService::import_anchor(&mut db, &package).is_err());
    }

    #[test]
    fn rejects_unsupported_algorithm() {
        let mut db = ConnectionFactory::new_for_test().unwrap();
        let value = serde_json::json!({
            "format": "provisioning-v1",
            "key_id": "lk-test",
            "algorithm": "RSA",
            "public_key": crate::infrastructure::licensing::encoding::encode_base64url(&[0u8; 32]),
        });
        assert!(TrustAnchorService::import_anchor(&mut db, &value.to_string()).is_err());
    }

    #[test]
    fn anchor_install_records_event() {
        let mut db = ConnectionFactory::new_for_test().unwrap();
        let licensor = Licensor::new("lk-test-0001");
        TrustAnchorService::import_anchor(&mut db, &licensor.provisioning_package()).unwrap();
        let events = LicensingEventsRepository::new(db.executor())
            .list_recent(10)
            .unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, "anchor-install");
        assert_eq!(events[0].key_id.as_deref(), Some("lk-test-0001"));
    }
}
