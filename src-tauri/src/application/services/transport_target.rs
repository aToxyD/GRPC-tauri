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
use std::path::{Path, PathBuf};

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

/// SEC-033: enumerate ALL authoritative UNIT targets for a fleet-level
/// WILAYA-issued export. The backend — never the renderer — decides the
/// target set: every registered `units` row is a delivery target.
///
/// Fail-closed semantics:
/// - zero registered UNITs → hard error (an export whose sole purpose is
///   artifact production must not succeed while producing nothing);
/// - any unit code unusable as a derived filename component → hard error
///   BEFORE any package is written or any sequence allocated.
pub fn resolve_fleet_unit_targets(executor: DbExecutor<'_>) -> AppResult<Vec<String>> {
    let units = executor.units().list_all_units()?;
    if units.is_empty() {
        return Err(AppError::BusinessLogic(
            BusinessLogicError::OperationNotPermitted {
                message: "لا توجد وحدات مُسجَّلة في السجل المحلي — لا أهداف لتصدير حزم المزامنة (رفض مغلق)".into(),
            },
        ));
    }
    let mut targets = Vec::with_capacity(units.len());
    for unit in &units {
        let code = unit.code.trim();
        validate_unit_code_as_path_component(code)?;
        targets.push(code.to_string());
    }
    Ok(targets)
}

/// Fail-closed validation for using a UNIT code as a derived filename
/// component (the trust-rotation convention). Blank codes and path separators
/// / traversal fragments are rejected.
pub fn validate_unit_code_as_path_component(code: &str) -> AppResult<()> {
    if code.is_empty()
        || code.contains('/')
        || code.contains('\\')
        || code.contains("..")
    {
        return Err(AppError::BusinessLogic(
            BusinessLogicError::OperationNotPermitted {
                message: format!("رمز الوحدة غير صالح كمكوّن اسم ملف مُشتق: {code} — رفض مغلق"),
            },
        ));
    }
    Ok(())
}

/// Derive the per-target output path for one UNIT target (canonical owner of
/// the trust-rotation naming convention).
///
/// Single-target emission keeps the operator-requested path unchanged;
/// multi-target emission suffixes each artifact with `-<unit_code>` so the
/// offline distribution set is self-describing.
pub fn derive_per_target_artifact_path(
    requested: &Path,
    target_code: &str,
    target_count: usize,
) -> PathBuf {
    if target_count <= 1 {
        return requested.to_path_buf();
    }
    // Non-UTF8 or missing stem/extension are degenerate path shapes, not
    // runtime errors — the deterministic defaults below keep the derived
    // artifact name well-formed (explicit match, no silent Option skip).
    let stem = match requested.file_stem().and_then(|s| s.to_str()) {
        Some(s) => s.to_string(),
        None => "package".to_string(),
    };
    let ext = match requested.extension().and_then(|s| s.to_str()) {
        Some(s) => s.to_string(),
        None => "sync".to_string(),
    };
    requested.with_file_name(format!("{stem}-{target_code}.{ext}"))
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

    fn seed_fleet(db: &Database, codes: &[&str]) {
        for (i, code) in codes.iter().enumerate() {
            db.executor()
                .units()
                .upsert_raw_unit(
                    &format!("22222222-3333-4333-8444-{:012}", i + 1),
                    code,
                    &format!("وحدة {code}"),
                    "16",
                    "2026-08-24T00:00:00Z",
                )
                .expect("seed fleet unit row");
        }
    }

    #[test]
    fn fleet_targets_enumerate_all_authoritative_units() {
        let db = ConnectionFactory::new_for_test().unwrap();
        seed_fleet(&db, &["U-B", "U-A", " U-C "]);

        let targets = resolve_fleet_unit_targets(db.executor()).unwrap();
        // Emission order is the repository's deterministic listing order;
        // the test pins membership, not a collation detail.
        let mut got = targets.clone();
        got.sort();
        assert_eq!(got, vec!["U-A", "U-B", "U-C"]);
    }

    #[test]
    fn fleet_targets_fail_closed_on_zero_registered_units() {
        let db = ConnectionFactory::new_for_test().unwrap();
        assert!(resolve_fleet_unit_targets(db.executor()).is_err());
    }

    #[test]
    fn fleet_targets_fail_closed_before_emission_on_unsafe_code() {
        let db = ConnectionFactory::new_for_test().unwrap();
        seed_fleet(&db, &["U-1", "../escape"]);
        // A single unsafe authoritative code aborts the whole enumeration.
        assert!(resolve_fleet_unit_targets(db.executor()).is_err());
    }

    #[test]
    fn unit_code_path_component_validation_rejects_unsafe_shapes() {
        assert!(validate_unit_code_as_path_component("U-9").is_ok());
        assert!(validate_unit_code_as_path_component("").is_err());
        assert!(validate_unit_code_as_path_component("a/b").is_err());
        assert!(validate_unit_code_as_path_component("a\\b").is_err());
        assert!(validate_unit_code_as_path_component("..").is_err());
        assert!(validate_unit_code_as_path_component("a..b").is_err());
    }

    #[test]
    fn per_target_artifact_path_keeps_requested_path_for_single_target() {
        let requested = Path::new("/tmp/out/catalog.sync");
        assert_eq!(
            derive_per_target_artifact_path(requested, "U-9", 1),
            requested.to_path_buf()
        );
    }

    #[test]
    fn per_target_artifact_path_suffixes_code_for_multi_target() {
        let requested = Path::new("/tmp/out/catalog.sync");
        assert_eq!(
            derive_per_target_artifact_path(requested, "U-1", 2),
            Path::new("/tmp/out/catalog-U-1.sync").to_path_buf()
        );
        assert_eq!(
            derive_per_target_artifact_path(requested, "U-2", 2),
            Path::new("/tmp/out/catalog-U-2.sync").to_path_buf()
        );
    }

    #[test]
    fn per_target_artifact_path_defaults_degenerate_shapes() {
        let no_ext = Path::new("/tmp/out/catalog");
        assert_eq!(
            derive_per_target_artifact_path(no_ext, "U-1", 3),
            Path::new("/tmp/out/catalog-U-1.sync").to_path_buf()
        );
        let dotfile = Path::new("/tmp/out/.sync");
        let derived = derive_per_target_artifact_path(dotfile, "U-1", 3);
        let name = match derived.file_name().and_then(|f| f.to_str()) {
            Some(n) => n.to_string(),
            None => panic!("derived name must be valid UTF-8"),
        };
        assert!(name.contains("-U-1."), "{name}");
    }
}
