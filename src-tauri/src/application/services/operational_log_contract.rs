//! Stable structured operational event contracts (deterministic assertions only).
//!
//! No JSON schema engine — constants + mandatory field names for log targets.

#[derive(Debug, Clone, Copy)]
pub struct LogEventContract {
    pub code: &'static str,
    pub target: &'static str,
    pub prefix: &'static str,
    pub mandatory_fields: &'static [&'static str],
}

pub const FISCAL_CLOSE_START: LogEventContract = LogEventContract {
    code: "FISCAL_CLOSE_START",
    target: "grpc::fiscal",
    prefix: "[FISCAL_CLOSE_START]",
    mandatory_fields: &["year", "next_year", "user_id"],
};

pub const FISCAL_CLOSE_SUCCESS: LogEventContract = LogEventContract {
    code: "FISCAL_CLOSE_SUCCESS",
    target: "grpc::fiscal",
    prefix: "[FISCAL_CLOSE_SUCCESS]",
    mandatory_fields: &["year", "next_year", "snapshot_count", "duration_ms"],
};

pub const FISCAL_CLOSE_FAILED: LogEventContract = LogEventContract {
    code: "FISCAL_CLOSE_FAILED",
    target: "grpc::fiscal",
    prefix: "[FISCAL_CLOSE_FAILED]",
    mandatory_fields: &["year", "next_year", "user_id", "err"],
};

pub const FISCAL_IMPORT_REJECTED: LogEventContract = LogEventContract {
    code: "FISCAL_IMPORT_REJECTED",
    target: "grpc::sync",
    prefix: "[FISCAL_IMPORT_REJECTED]",
    mandatory_fields: &[
        "source_node",
        "package_id",
        "incoming_year",
        "current_year",
        "reason",
    ],
};

pub const INTEGRITY_OPERATION_BLOCKED: LogEventContract = LogEventContract {
    code: "INTEGRITY_OPERATION_BLOCKED",
    target: "grpc::integrity",
    prefix: "[INTEGRITY_OPERATION_BLOCKED]",
    mandatory_fields: &["state", "ops"],
};

pub const BACKUP_RESTORE_REJECTED: LogEventContract = LogEventContract {
    code: "BACKUP_RESTORE_REJECTED",
    target: "grpc::backup",
    prefix: "[BACKUP_RESTORE_REJECTED]",
    mandatory_fields: &["reason"],
};

pub const OPERATIONAL_ANOMALY_ARCHIVE_REJECTED: LogEventContract = LogEventContract {
    code: "ARCHIVE_REJECTED",
    target: "grpc::operational_anomaly",
    prefix: "[ARCHIVE_REJECTED]",
    mandatory_fields: &["year", "reason"],
};

pub const ALL_CRITICAL_CONTRACTS: &[LogEventContract] = &[
    FISCAL_CLOSE_START,
    FISCAL_CLOSE_SUCCESS,
    FISCAL_CLOSE_FAILED,
    FISCAL_IMPORT_REJECTED,
    INTEGRITY_OPERATION_BLOCKED,
    BACKUP_RESTORE_REJECTED,
    OPERATIONAL_ANOMALY_ARCHIVE_REJECTED,
];

/// Format string used at the call site must include every mandatory field name.
pub fn assert_log_format_includes_fields(format: &str, contract: &LogEventContract) {
    for field in contract.mandatory_fields {
        assert!(
            format.contains(field),
            "contract {} missing field '{}' in format",
            contract.code,
            field
        );
    }
}

/// Deterministic serialization of contract metadata for snapshot tests.
pub fn contract_snapshot(contract: &LogEventContract) -> String {
    format!(
        "{}|{}|{}|{}",
        contract.code,
        contract.target,
        contract.prefix,
        contract.mandatory_fields.join(",")
    )
}
