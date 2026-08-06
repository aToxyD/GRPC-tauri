//! Import-time checks tying package metadata to the Wilaya operator’s selected unit.

use crate::application::sync::SyncPackage;
use crate::application::usecases::exports::types::MonthlySummaryExportDataset;
use crate::models::Unit;

/// True when `source_node_id` can legitimately name the exporting node for this Wilaya import.
///
/// - **Unit node exports** use [`crate::infrastructure::sync::resolve_export_source_node_id`]:
///   **`units.id`** when the local row matches `settings.unit_name` + `wilaya_code`, else the legacy
///   display name from [`crate::infrastructure::sync::infer_source_node_id`].
/// - **Wilaya node exports** use the wilaya code as `source_node_id`; then the same code must match
///   the importer’s wilaya and the selected unit must belong to that wilaya (checked separately).
pub fn monthly_summary_source_allowed_for_unit(
    package: &SyncPackage<MonthlySummaryExportDataset>,
    unit: &Unit,
    importer_wilaya_code: &str,
) -> bool {
    source_id_allowed_for_unit(
        package.metadata.source_node_id.as_str(),
        unit,
        importer_wilaya_code,
    )
}

/// Generic unit-bound provenance check reused by multiple package kinds.
pub fn source_id_allowed_for_unit(
    source_node_id: &str,
    unit: &Unit,
    importer_wilaya_code: &str,
) -> bool {
    let s = source_node_id.trim();
    if s.is_empty() {
        return false;
    }
    if s == unit.id.as_str() || s == unit.name.as_str() || s == unit.code.as_str() {
        return true;
    }
    s == importer_wilaya_code && unit.wilaya_code == importer_wilaya_code
}

/// Products packages are expected to be distributed by the Wilaya (coordinator) node.
pub fn products_source_allowed_for_unit(importer_wilaya_code: &str, source_node_id: &str) -> bool {
    let w = importer_wilaya_code.trim();
    let s = source_node_id.trim();
    !w.is_empty() && !s.is_empty() && w == s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::sync::{PackageId, SchemaVersion, SyncPackage, SyncPackageMetadata};
    use crate::application::usecases::exports::types::MonthlySummaryExportDataset;
    use crate::models::{DailyDetailSyncSnapshot, MonthlySummary};
    use chrono::{NaiveDate, TimeZone, Utc};

    fn unit_sample() -> Unit {
        Unit {
            id: "u-1".into(),
            code: "C01".into(),
            name: "Alpha Base".into(),
            wilaya_code: "16".into(),
            user_id: None,
            created_at: Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap(),
        }
    }

    fn pkg_with_source(source: &str) -> SyncPackage<MonthlySummaryExportDataset> {
        SyncPackage {
            metadata: SyncPackageMetadata {
                schema_version: SchemaVersion::V1,
                created_at: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
                source_node_id: source.into(),
                package_sequence: None,
                issuer_identity_id: None,
                package_id: PackageId("p".into()),
                signature_version: None,
                signing_key_id: None,
                integrity_hash: None,
                signature: None,
            },
            payload: MonthlySummaryExportDataset {
                summary: MonthlySummary {
                    month: 1,
                    year: 2026,
                    total_beneficiaries: 0,
                    total_consumption_value: 0.0,
                    breakfast_average: 0.0,
                    lunch_average: 0.0,
                    dinner_average: 0.0,
                    daily_average: 0.0,
                    report_count: 0,
                },
                daily_detail_rows: vec![DailyDetailSyncSnapshot {
                    date: NaiveDate::from_ymd_opt(2026, 1, 10).unwrap(),
                    total_daily_beneficiaries: 0,
                    total_daily_cost: 0.0,
                    breakfast_average: 0.0,
                    lunch_average: 0.0,
                    dinner_average: 0.0,
                    daily_average: 0.0,
                }],
            },
        }
    }

    #[test]
    fn allows_unit_name_match() {
        let u = unit_sample();
        let p = pkg_with_source("Alpha Base");
        assert!(monthly_summary_source_allowed_for_unit(&p, &u, "16"));
    }

    #[test]
    fn allows_wilaya_code_when_unit_in_same_wilaya() {
        let u = unit_sample();
        let p = pkg_with_source("16");
        assert!(monthly_summary_source_allowed_for_unit(&p, &u, "16"));
    }

    #[test]
    fn rejects_foreign_wilaya_source_mismatch() {
        let u = unit_sample();
        let p = pkg_with_source("99");
        assert!(!monthly_summary_source_allowed_for_unit(&p, &u, "16"));
    }
}
