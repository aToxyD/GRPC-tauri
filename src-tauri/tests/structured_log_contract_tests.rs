//! Structured operational log contract tests — stable codes, fields, targets.

use grpc_lib::application::services::operational_log_contract::{
    assert_log_format_includes_fields, contract_snapshot, ALL_CRITICAL_CONTRACTS,
    FISCAL_CLOSE_START, FISCAL_CLOSE_SUCCESS, FISCAL_IMPORT_REJECTED,
};

#[test]
fn critical_events_have_stable_codes_and_prefixes() {
    for c in ALL_CRITICAL_CONTRACTS {
        assert!(!c.code.is_empty());
        assert!(c.prefix.starts_with('['));
        assert!(c
            .prefix
            .contains(c.code.trim_start_matches('[').trim_end_matches(']')));
        assert!(!c.target.is_empty());
        assert!(!c.mandatory_fields.is_empty());
    }
}

#[test]
fn fiscal_close_log_formats_include_mandatory_fields() {
    assert_log_format_includes_fields(
        "[FISCAL_CLOSE_START] year={} next_year={} user_id={}",
        &FISCAL_CLOSE_START,
    );
    assert_log_format_includes_fields(
        "[FISCAL_CLOSE_SUCCESS] year={} next_year={} snapshot_count={} duration_ms={}",
        &FISCAL_CLOSE_SUCCESS,
    );
}

#[test]
fn fiscal_import_rejected_format_includes_mandatory_fields() {
    assert_log_format_includes_fields(
        "[FISCAL_IMPORT_REJECTED] source_node={} package_id={} incoming_year={} current_year={} reason={}",
        &FISCAL_IMPORT_REJECTED,
    );
}

#[test]
fn contract_snapshots_are_deterministic() {
    let s1 = contract_snapshot(&FISCAL_CLOSE_START);
    let s2 = contract_snapshot(&FISCAL_CLOSE_START);
    assert_eq!(s1, s2);
    assert!(s1.contains("FISCAL_CLOSE_START"));
    assert!(s1.contains("grpc::fiscal"));
}
