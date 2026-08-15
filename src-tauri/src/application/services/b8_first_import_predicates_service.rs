//! B8 First-Import Predicates (ADR-0045, RFC 2026-08-04 §3.12)
//!
//! Fail-closed gate for the anonymous-at-command-authorization-layer first
//! `identity_access` import on a fresh UNIT node. ALL predicates must hold;
//! any failure rejects the import before any state mutation.
//!
//! Self-terminating: once the first import succeeds, a canonical Admin
//! account exists and `no_active_admin` fails closed — the exemption cannot
//! be replayed on a provisioned node.

use crate::domain::identity::ports::IdentityStorePort;
use crate::domain::identity::SubjectType;
use crate::errors::{AppError, ValidationError};
use crate::repositories::{DbExecutor, RepositoryProvider};

/// Verdict of the four B8 first-import predicates.
pub struct B8FirstImportVerdict {
    /// A valid ACTIVE WILAYA trust anchor is installed locally.
    pub anchor_installed: bool,
    /// The package issuer is the locally installed WILAYA anchor.
    pub anchor_is_issuer: bool,
    /// The package payload targets the local unit (unit_code match).
    pub unit_code_matches: bool,
    /// No canonical Admin account exists yet.
    pub no_active_admin: bool,
}

impl B8FirstImportVerdict {
    /// ALL predicates must hold for the first-import exemption (fail-closed).
    pub fn all_hold(&self) -> bool {
        self.anchor_installed
            && self.anchor_is_issuer
            && self.unit_code_matches
            && self.no_active_admin
    }
}

pub struct B8FirstImportPredicatesService;

impl B8FirstImportPredicatesService {
    /// Evaluate all predicates against the current persisted state.
    ///
    /// The anchor query is the ACTIVE WILAYA certificate row (never a caller
    /// supplied certificate): `anchor == issuer` therefore binds the package
    /// issuer to the locally installed trust anchor, closing the
    /// cross-WILAYA acceptance vector.
    pub fn evaluate(
        executor: &DbExecutor<'_>,
        payload_unit_code: &str,
        issuer_identity_id: Option<&str>,
        local_unit_code: Option<&str>,
    ) -> Result<B8FirstImportVerdict, AppError> {
        let anchor = executor
            .identity_store()
            .get_active_by_subject_type(SubjectType::Wilaya)?;

        let anchor_installed = anchor.is_some();
        let anchor_is_issuer = match (anchor.as_ref(), issuer_identity_id) {
            (Some(a), Some(issuer)) => a.identity_id.to_string() == issuer,
            _ => false,
        };
        let unit_code_matches = match local_unit_code {
            Some(c) => c == payload_unit_code,
            None => false,
        };
        let no_active_admin = executor.users().count_active_admins()? == 0;

        Ok(B8FirstImportVerdict {
            anchor_installed,
            anchor_is_issuer,
            unit_code_matches,
            no_active_admin,
        })
    }

    /// First failing predicate, in evaluation order, as a deterministic
    /// Arabic operator-facing message.
    pub fn rejection_message(verdict: &B8FirstImportVerdict) -> &'static str {
        if !verdict.anchor_installed {
            "لا يمكن استيراد حزمة الحسابات قبل تثبيت مرساة الثقة المحلية (شهادة الولاية النشطة)"
        } else if !verdict.anchor_is_issuer {
            "مُصدِر الحزمة ليس مرساة الثقة المحلية المثبتة"
        } else if !verdict.unit_code_matches {
            "رمز الوحدة في الحزمة لا يطابق الوحدة المحلية"
        } else {
            "يوجد حساب مسؤول نشط بالفعل — الاستيراد الأولي (B8) غير متاح"
        }
    }

    /// ADR-0044 V2 `.unit` acceptance gate (Trust-First model, RFC §3.10):
    ///
    /// - The ACTIVE WILAYA trust anchor is installed locally (Root-verified);
    /// - the package issuer IS the installed anchor (no cross-WILAYA
    ///   acceptance);
    /// - the package is the first signed artifact from this issuer on this
    ///   node (empty transport ledger) with the fixed bootstrap sequence 1
    ///   (A44-08).
    ///
    /// The `.unit` is a one-time bootstrap artifact OUTSIDE the Transport
    /// Guard ordering domain: it never advances the per-issuer ledger, so the
    /// first `identity_access` import still opens the ledger at sequence 1
    /// (A45-06). A re-imported `.unit` on a node that already holds ledger
    /// state fails closed — recovery is the governed reprovisioning process
    /// (A44-12), never a V2→V1 shortcut.
    pub fn verify_unit_v2_acceptance(
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
                rejection(
                    "لا يمكن قبول حزمة العقدة V2 قبل تثبيت مرساة الثقة المحلية (شهادة الولاية النشطة)",
                )
            })?;

        if anchor.identity_id.to_string() != issuer_identity_id {
            return Err(rejection("مُصدِر حزمة العقدة ليس مرساة الثقة المحلية المثبتة"));
        }
        if package_sequence != Some(1) {
            return Err(rejection(
                "أول حزمة عقدة V2 يجب أن تحمل رقم التسلسل 1 (A44-08)",
            ));
        }

        let last = executor
            .sync_applied_packages()
            .last_applied_sequence_for_issuer(issuer_identity_id)?;
        if last.is_some() {
            return Err(rejection(
                "حزمة العقدة V2 تُقبل مرة واحدة فقط على العقدة الجديدة",
            ));
        }
        Ok(())
    }
}