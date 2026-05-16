//! Best-effort node label for package provenance (not a cryptographic identity).

use crate::errors::AppResult;
use crate::models::{NodeType, Settings};
use crate::repositories::executor::DbExecutor;
use crate::repositories::UnitRepository;

pub fn infer_source_node_id(settings: &Settings) -> String {
    match settings.node_type {
        NodeType::Wilaya => settings
            .wilaya_code
            .clone()
            .unwrap_or_else(|| "unknown_wilaya".into()),
        NodeType::Unit => settings
            .unit_name
            .clone()
            .unwrap_or_else(|| "unknown_unit".into()),
    }
}

/// Prefer stable **`units.id`** on UNIT nodes when the row exists locally (matches Wilaya import dropdown).
/// Falls back to [`infer_source_node_id`] (historically `settings.unit_name`).
pub fn resolve_export_source_node_id(
    executor: DbExecutor<'_>,
    settings: &Settings,
) -> AppResult<String> {
    match settings.node_type {
        NodeType::Wilaya => Ok(infer_source_node_id(settings)),
        NodeType::Unit => {
            let Some(name) = settings
                .unit_name
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
            else {
                return Ok(infer_source_node_id(settings));
            };
            let Some(wc) = settings
                .wilaya_code
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
            else {
                return Ok(infer_source_node_id(settings));
            };
            let id = UnitRepository::new(executor).find_unit_id_by_name_and_wilaya(name, wc)?;
            Ok(id.unwrap_or_else(|| infer_source_node_id(settings)))
        }
    }
}
