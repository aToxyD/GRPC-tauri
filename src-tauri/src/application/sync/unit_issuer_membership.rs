//! UNIT issuer membership predicate (ADR-0046 §3 I3).
//!
//! For UNIT-issued data packages, the authenticated certificate subject must
//! resolve to a local `units` row belonging to the importer's WILAYA:
//!
//! ```text
//! cert.subject_id
//!       ↓
//! units.get_unit(subject_id)
//!       ↓
//! Unit.wilaya_code == importer_wilaya_code (trusted local settings)
//! ```
//!
//! Both inputs are trusted local persisted state: the certificate subject is
//! resolved by the verifier from the Identity Store (never renderer-supplied),
//! and `importer_wilaya_code` comes from `settings.wilaya_code`. Fail-closed:
//! a missing unit row and a foreign-WILAYA unit are both rejected. There is no
//! fallback behavior.

use crate::errors::{AppError, AppResult, ValidationError};
use crate::repositories::{DbExecutor, RepositoryProvider};

/// Verify that the authenticated issuer certificate's `subject_id` identifies
/// a UNIT belonging to the importer's local WILAYA.
///
/// Must be called only AFTER the package Ed25519 signature has been verified
/// (signature-first ordering, ADR-0046 §5 step 8a).
pub fn verify_unit_issuer_membership(
    executor: DbExecutor<'_>,
    issuer_cert_subject_id: &str,
    importer_wilaya_code: &str,
) -> AppResult<()> {
    let reject = |message: &str| {
        AppError::Validation(ValidationError::InvalidFormat {
            field: "issuer_identity_id".into(),
            message: message.into(),
        })
    };

    let subject_id = issuer_cert_subject_id.trim();
    if subject_id.is_empty() {
        return Err(reject("هوية المُصدِر فارغة"));
    }

    let unit = executor.units().get_unit(subject_id)?.ok_or_else(|| {
        reject(&format!(
            "هوية المُصدِر «{subject_id}» لا تطابق أي وحدة مسجلة"
        ))
    })?;

    if unit.wilaya_code.trim() != importer_wilaya_code.trim() {
        return Err(reject(&format!(
            "وحدة المُصدِر «{subject_id}» لا تنتمي إلى ولاية المستورد ({} != {})",
            unit.wilaya_code.trim(),
            importer_wilaya_code.trim()
        )));
    }

    Ok(())
}