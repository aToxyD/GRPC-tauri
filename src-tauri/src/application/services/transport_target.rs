//! Canonical transport-target resolution for V2 package exports.
//!
//! ADR-0053 §3.3 (normative): the allocation key `target_node_id` MUST come
//! from authoritative local server-side state — NEVER from renderer-provided
//! arbitrary identifiers:
//!
//! - UNIT recipient   (WILAYA-issued kinds: products / admin_access / trust)
//!   → `units.code`, validated against authoritative `units` rows;
//! - WILAYA recipient (UNIT-issued kinds: daily_report / monthly_summary /
//!   stock_movements) → `settings.wilaya_code`.
//!
//! Both resolvers fail closed: an empty/unresolvable target aborts the export
//! BEFORE any sequence allocation (no ledger movement). There is no fallback
//! to `unit_name`, no placeholder codes, and `source_node_id`
//! (provenance-only, see `infrastructure::sync::source_node`) is never used as
//! a transport target.

use crate::errors::{AppError, AppResult, BusinessLogicError, ValidationError};
use crate::models::{NodeType, Settings};
use crate::repositories::executor::DbExecutor;
use crate::repositories::RepositoryProvider;

/// Resolve the transport target for a UNIT-issued export: the authoritative
/// local `settings.wilaya_code`. Fail-closed when unset/blank.
pub fn resolve_wilaya_transport_target(
    executor: DbExecutor<'_>,
    settings: &Settings,
) -> AppResult<String> {
    let _ = executor;
    if settings.node_type != NodeType::Unit {
        return Err(AppError::BusinessLogic(
            BusinessLogicError::OperationNotPermitted {
                message: "هدف نقل حزمة WILAYA لا يُحدَّد إلا من عقدة UNIT".into(),
            },
        ));
    }
    let code = settings
        .wilaya_code
        .as_deref()
        .map(str::trim)
        .filter(|c| !c.is_empty())
        .ok_or_else(|| {
            AppError::Validation(ValidationError::InvalidFormat {
                field: "target_node_id".into(),
                message: "رمز الولاية المحلي غير مضبوط — لا يمكن تحديد هدف نقل الحزمة".into(),
            })
        })?;
    Ok(code.to_string())
}

/// Resolve the transport target for a WILAYA-issued export: the selected UNIT
/// code validated against the authoritative `units` rows. Fail-closed when the
/// code does not resolve to an existing unit row. Never falls back to
/// `unit_name`.
pub fn resolve_unit_transport_target(
    executor: DbExecutor<'_>,
    unit_code: &str,
) -> AppResult<String> {
    let code = unit_code.trim();
    if code.is_empty() {
        return Err(AppError::Validation(ValidationError::InvalidFormat {
            field: "target_node_id".into(),
            message: "رمز الوحدة المستهدفة فارغ".into(),
        }));
    }
    let unit = executor
        .units()
        .get_unit_by_code(code)?
        .ok_or_else(|| {
            AppError::BusinessLogic(BusinessLogicError::OperationNotPermitted {
                message: format!("الوحدة المستهدفة غير موجودة في السجل المحلي: {code} — رفض مغلق"),
            })
        })?;
    Ok(unit.code)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{ConnectionFactory, Database};

    fn unit_settings(wilaya: Option<&str>) -> Settings {
        Settings {
            id: 1,
            node_type: NodeType::Unit,
            unit_name: Some("وحدة الجزائر الوسطى".into()),
            unit_code: None,
            current_year: 2026,
            wilaya_code: wilaya.map(str::to_string),
            wilaya_name: Some("الجزائر".into()),
            configured: true,
        }
    }

    fn insert_test_unit(db: &Database) {
        db.executor()
            .units()
            .upsert_raw_unit(
                "11111111-2222-4333-8444-555555555555",
                "U-100",
                "وحدة 100",
                "16",
                "2026-08-24T00:00:00Z",
            )
            .expect("seed authoritative unit row");
    }

    #[test]
    fn wilaya_target_resolves_from_authoritative_settings() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let s = unit_settings(Some("16"));
        assert_eq!(
            resolve_wilaya_transport_target(db.executor(), &s).unwrap(),
            "16"
        );
    }

    #[test]
    fn wilaya_target_fails_closed_when_unset_or_blank() {
        let db = ConnectionFactory::new_for_test().unwrap();
        assert!(resolve_wilaya_transport_target(db.executor(), &unit_settings(None)).is_err());
        assert!(
            resolve_wilaya_transport_target(db.executor(), &unit_settings(Some("  "))).is_err()
        );
    }

    #[test]
    fn wilaya_target_rejected_from_non_unit_nodes() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let mut s = unit_settings(Some("16"));
        s.node_type = NodeType::Wilaya;
        assert!(resolve_wilaya_transport_target(db.executor(), &s).is_err());
    }

    #[test]
    fn unit_target_resolves_only_against_authoritative_rows() {
        let db = ConnectionFactory::new_for_test().unwrap();
        insert_test_unit(&db);

        assert_eq!(
            resolve_unit_transport_target(db.executor(), " U-100 ").unwrap(),
            "U-100"
        );
        // Unknown code → fail closed (no name fallback).
        assert!(resolve_unit_transport_target(db.executor(), "U-404").is_err());
        assert!(resolve_unit_transport_target(db.executor(), "").is_err());
        assert!(resolve_unit_transport_target(db.executor(), "   ").is_err());
        // The display NAME must never resolve as a target.
        assert!(resolve_unit_transport_target(db.executor(), "وحدة 100").is_err());
    }
}
