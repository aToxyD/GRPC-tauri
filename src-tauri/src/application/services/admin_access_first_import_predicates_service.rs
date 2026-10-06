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
//! Durable initialization latch (ADR-0063 §8.1): `no_active_admin` reports
//! whether a canonical `admin` row bound to the local UNIT node exists —
//! without any `deleted` filter. Once the first import succeeds, that row
//! exists and no supported operation removes it (disable is soft, `deleted =
//! 1`, and is ignored by the latch), so initialization completion is
//! monotonic and the exemption cannot be re-opened on a provisioned node
//! (D5, F15).

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
    /// No canonical `admin` row is bound to the local UNIT node yet — the
    /// ADR-0063 §8.1 existence latch, evaluated without any `deleted` filter
    /// (node-bound and deletion-insensitive).
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
    ///
    /// `local_unit_code` is the authoritative local UNIT code captured by the
    /// import path (ADR-0063 §8.1); `no_active_admin` answers the canonical
    /// local-admin existence question against it. Fail-closed: a missing or
    /// empty local UNIT code can never satisfy the latch, so the exemption is
    /// never admitted without an authoritative local node identity.
    pub fn evaluate(
        executor: &DbExecutor<'_>,
        issuer_identity_id: Option<&str>,
        local_unit_code: Option<&str>,
    ) -> Result<AdminAccessFirstImportVerdict, AppError> {
        let anchor = executor
            .identity_store()
            .get_active_by_subject_type(SubjectType::Wilaya)?;

        let anchor_installed = anchor.is_some();
        let anchor_is_issuer = match (anchor.as_ref(), issuer_identity_id) {
            (Some(a), Some(issuer)) => a.identity_id.to_string() == issuer,
            _ => false,
        };
        // ADR-0063 §8.1: the durable existence latch — canonical `admin` row
        // bound to the local UNIT node, deletion-insensitive. Fail closed on
        // a missing/empty local UNIT code (no authoritative latch input).
        let no_active_admin = match local_unit_code.map(str::trim) {
            Some(code) if !code.is_empty() => !executor.users().admin_exists_for_node(code)?,
            _ => false,
        };

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
            "يوجد حساب مسؤول بالفعل — الاستيراد الأولي غير متاح"
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
