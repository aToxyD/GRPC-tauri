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

    /// ADR-0051 §8 first-import acceptance gate. The transport-sequence
    /// bootstrap (SEC-056D/SEC-057 retirement) is removed: no sequence-1
    /// requirement and no per-issuer transport ledger check. The only
    /// remaining fail-closed predicate here is the issuer-pinning check — the
    /// package issuer MUST be the locally installed ACTIVE WILAYA anchor
    /// (cross-WILAYA rejection). The "first import only" property is carried
    /// by the `no_active_admin` predicate and exact `package_id` dedup.
    pub fn verify_first_import_issuer(
        executor: &DbExecutor<'_>,
        issuer_identity_id: &str,
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

        Ok(())
    }
}
