//! Append-only import reproducibility metadata — does not alter sync envelope or crypto.

use crate::errors::AppError;
use crate::repositories::executor::DbExecutor;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReproducibilityRecord {
    pub package_id: String,
    pub package_kind: String,
    pub imported_at: String,
    pub imported_by: String,
    pub validation_state: String,
    pub source_integrity_state: Option<String>,
    pub rejected_records_count: i64,
}

pub struct ImportReproducibilityService<'a> {
    executor: DbExecutor<'a>,
}

impl<'a> ImportReproducibilityService<'a> {
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }

    pub fn record_import_metadata(
        &self,
        record: &ImportReproducibilityRecord,
    ) -> Result<i64, AppError> {
        use crate::repositories::RepositoryProvider;
        let id = self
            .executor
            .import_audit_events()
            .record_reproducibility_metadata(
                &record.package_id,
                &record.package_kind,
                &record.imported_at,
                &record.imported_by,
                &record.validation_state,
                record.source_integrity_state.as_deref(),
                record.rejected_records_count,
            )?;

        log::info!(
            target: "grpc::import_export",
            "[IMPORT_REPRO_METADATA] package_id={} validation={} rejected={}",
            record.package_id,
            record.validation_state,
            record.rejected_records_count
        );
        Ok(id)
    }
}

impl crate::architecture::Service for ImportReproducibilityService<'_> {}
