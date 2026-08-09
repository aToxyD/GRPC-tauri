//! Licensing commands (ADR-0042).
//!
//! Thin IPC handlers: authorization guard + service dispatch only. The node's
//! public key (subject-binding input) is resolved here from the node key store;
//! all policy and semantic decisions live in `application/licensing`.

use tauri::State;

use crate::app::state::AppState;
use crate::application::authz::Action;
use crate::application::licensing::{LicenseVerificationService, TrustAnchorService};
use crate::commands::common::{
    db_mut_or_command_error, db_ref_or_command_error, resolve_node_public_key,
};
use crate::commands::guards::authorize_command;
use crate::errors::into_command_error;
use crate::models::{
    ImportAnchorResult, ImportLicenseResult, LicensingStatus, VerifyLicenseReport,
};

/// Read-only live licensing projection (anchor + stored licenses + gate state).
/// Exempt from the enforcement gate (`ReadLicensingStatus`) so operators can
/// inspect and diagnose licensing state even while the gate denies operations.
#[tauri::command]
pub fn get_licensing_status(state: State<AppState>) -> Result<LicensingStatus, String> {
    let (_session, _settings) =
        authorize_command(&state, Action::ReadLicensingStatus, None).map_err(into_command_error)?;
    state.touch_session();
    let guard = state.get_db().map_err(into_command_error)?;
    let db = db_ref_or_command_error(guard.as_ref())?;
    LicenseVerificationService::status(db, resolve_node_public_key()).map_err(into_command_error)
}

/// Install or rotate the single active licensing trust anchor from a
/// `provisioning-v1` package. Exempt from the enforcement gate so the anchor
/// can be installed on a node that holds no license yet.
#[tauri::command]
pub fn import_trust_anchor(
    state: State<AppState>,
    package_json: String,
) -> Result<ImportAnchorResult, String> {
    let (_session, _settings) =
        authorize_command(&state, Action::ManageLicensing, None).map_err(into_command_error)?;
    state.touch_session();
    let mut guard = state.get_db().map_err(into_command_error)?;
    let db = db_mut_or_command_error(guard.as_mut())?;
    TrustAnchorService::import_anchor(db, &package_json).map_err(into_command_error)
}

/// Import a signed license artifact: full verification pipeline (§4 steps
/// 1–6) + semantic validation + subject binding, then persist the derived
/// view (full overwrite). Exempt from the enforcement gate.
#[tauri::command]
pub fn import_license(
    state: State<AppState>,
    artifact_json: String,
) -> Result<ImportLicenseResult, String> {
    let (_session, _settings) =
        authorize_command(&state, Action::ManageLicensing, None).map_err(into_command_error)?;
    state.touch_session();
    let mut guard = state.get_db().map_err(into_command_error)?;
    let db = db_mut_or_command_error(guard.as_mut())?;
    LicenseVerificationService::import_license(db, resolve_node_public_key(), &artifact_json)
        .map_err(into_command_error)
}

/// Deterministic pre-import check: runs the FULL pipeline without persisting
/// anything. Read-only (`ReadLicensingStatus`).
#[tauri::command]
pub fn dry_run_verify_license(
    state: State<AppState>,
    artifact_json: String,
) -> Result<ImportLicenseResult, String> {
    let (_session, _settings) =
        authorize_command(&state, Action::ReadLicensingStatus, None).map_err(into_command_error)?;
    state.touch_session();
    let guard = state.get_db().map_err(into_command_error)?;
    let db = db_ref_or_command_error(guard.as_ref())?;
    LicenseVerificationService::dry_run_verify(db, resolve_node_public_key(), &artifact_json)
        .map_err(into_command_error)
}

/// Deterministic re-verification of every stored license from the persisted
/// canonical bytes (ADR-0042 §3). Read-only.
#[tauri::command]
pub fn verify_license(state: State<AppState>) -> Result<VerifyLicenseReport, String> {
    let (_session, _settings) =
        authorize_command(&state, Action::ReadLicensingStatus, None).map_err(into_command_error)?;
    state.touch_session();
    let guard = state.get_db().map_err(into_command_error)?;
    let db = db_ref_or_command_error(guard.as_ref())?;
    LicenseVerificationService::reverify_stored(db, resolve_node_public_key())
        .map_err(into_command_error)
}
