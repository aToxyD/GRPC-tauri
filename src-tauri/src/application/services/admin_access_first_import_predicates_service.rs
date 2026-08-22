//! Admin-Access First-Import Predicates (ADR-0051 §8 — Accepted 2026-08-22).
//!
//! Fail-closed gate for the anonymous-at-command-authorization-layer first
//! `admin_access` import on a fresh UNIT node. ALL predicates must hold; any
//! failure rejects the import before any state mutation.
//!
//! This service is deliberately PARALLEL to [`super::b8_first_import_predicates_service::B8FirstImportPredicatesService`]:
//! the certified `identity_access` predicate set (including
//! `unit_code_matches`) remains untouched. ADR-0051 removes the target-binding
//! predicate because an `admin_access` package is fleet-wide and has no
//! target binding to check — issuer pinning over the V2 signature chain
//! (`anchor_is_issuer`) carries its protective role instead. This is not an
//! authentication weakening: authentication remains carried entirely by the
//! certificate ↔ signature verification performed before these predicates run.
//!
//! Self-terminating: once the first import succeeds, a canonical Admin
//! account exists and `no_active_admin` fails closed — the exemption cannot
//! be replayed on a provisioned node.

use crate::domain::identity::ports::IdentityStorePort;
use crate::domain::identity::SubjectType;
use crate::errors::{AppError, ValidationError};
use crate::repositories::{DbExecutor, RepositoryProvider};

/// Verdict of the three admin-access first-import predicates.
pub struct AdminAccessFirstImportVerdict {
    /// A valid ACTIVE WILAYA trust anchor is installed locally.
    pub anchor_installed: bool,
    /// The package issuer is the locally installed WILAYA anchor.
    pub anchor_is_issuer: bool,
    /// No canonical Admin account exists yet.
    pub no_active_admin: bool,
}

impl AdminAccessFirstImportVerdict {
    /// ALL predicates must hold for the first-import exemption (fail-closed).
    pub fn all_hold(&self) -> bool {
        self.anchor_installed && self.anchor_is_issuer && self.no_active_admin
    }
}

pub struct AdminAccessFirstImportPredicatesService;

impl AdminAccessFirstImportPredicatesService {
    /// Evaluate all predicates against the current persisted state.
    ///
    /// The anchor query is the ACTIVE WILAYA certificate row (never a caller
    /// supplied certificate): `anchor == issuer` therefore binds the package
    /// issuer to the locally installed trust anchor, closing the cross-WILAYA
    /// acceptance vector without any target binding.
    pub fn evaluate(
        executor: &DbExecutor<'_>,
        issuer_identity_id: Option<&str>,
    ) -> Result<AdminAccessFirstImportVerdict, AppError> {
        let anchor = executor
            .identity_store()
            .get_active_by_subject_type(SubjectType::Wilaya)?;

        let anchor_installed = anchor.is_some();
        let anchor_is_issuer = match (anchor.as_ref(), issuer_identity_id) {
            (Some(a), Some(issuer)) => a.identity_id.to_string() == issuer,
            _ => false,
        };
        let no_active_admin = executor.users().count_active_admins()? == 0;

        Ok(AdminAccessFirstImportVerdict {
            anchor_installed,
            anchor_is_issuer,
            no_active_admin,
        })
    }

    /// First failing predicate, in evaluation order, as a deterministic
    /// Arabic operator-facing message.
    pub fn rejection_message(verdict: &AdminAccessFirstImportVerdict) -> &'static str {
        if !verdict.anchor_installed {
            "لا يمكن استيراد حزمة حساب المدير قبل تثبيت مرساة الثقة المحلية (شهادة الولاية النشطة)"
        } else if !verdict.anchor_is_issuer {
            "مُصدِر الحزمة ليس مرساة الثقة المحلية المثبتة"
        } else {
            "يوجد حساب مسؤول نشط بالفعل — الاستيراد الأولي غير متاح"
        }
    }

    /// ADR-0051 §8 first-package acceptance gate for an EMPTY transport
    /// ledger, mirroring the certified `.unit` V2 shape (ADR-0044 A44-08 /
    /// ADR-0045 A45-06): on a node with no ledger state for this issuer, the
    /// only admissible first package carries sequence 1. Later packages are
    /// governed exclusively by the standard Transport Guard continuity rule,
    /// whose state is strictly local per node (fleet-wide broadcast property:
    /// every UNIT independently accepts the same sequence N).
    ///
    /// Fail-closed when the anchor is missing or the issuer is not the anchor.
    pub fn verify_first_package_sequence(
        executor: &DbExecutor<'_>,
        issuer_identity_id: &str,
        package_sequence: Option<u64>,
    ) -> Result<(), AppError> {
        let rejection = |message: &str| {
            AppError::Validation(ValidationError::InvalidFormat {
                field: "signature".into(),
                message: message.into(),
            })
        };

        let anchor = executor
            .identity_store()
            .get_active_by_subject_type(SubjectType::Wilaya)?
            .ok_or_else(|| {
                rejection("لا يمكن قبول حزمة حساب المدير قبل تثبيت مرساة الثقة المحلية")
            })?;

        if anchor.identity_id.to_string() != issuer_identity_id {
            return Err(rejection(
                "مُصدِر حزمة حساب المدير ليس مرساة الثقة المحلية المثبتة",
            ));
        }

        let last = executor
            .sync_applied_packages()
            .last_applied_sequence_for_issuer(issuer_identity_id)?;
        // Empty ledger ⇒ strict bootstrap: sequence MUST be exactly 1.
        // Non-empty ledger ⇒ the exemption never applies at all.
        if last.is_none() && package_sequence != Some(1) {
            return Err(rejection(
                "أول حزمة حساب مدير على العقدة يجب أن تحمل رقم التسلسل 1",
            ));
        }
        Ok(())
    }
}
