//! Settings and Configuration Commands

use crate::application::authz::Action;
use crate::application::services::SettingsService;
use crate::commands::common::db_ref_or_command_error;
use crate::commands::guards::authorize_command;
use crate::commands::types::AppState;
use crate::errors::into_command_error;
use crate::models::{Settings, WilayaNodeConfiguration};
use tauri::State;

/// Get system settings
#[tauri::command]
pub fn get_settings(state: State<AppState>) -> Result<Settings, String> {
    let guard = state.get_db().map_err(into_command_error)?;
    let db = db_ref_or_command_error(guard.as_ref())?;

    SettingsService::new(db.executor())
        .get_settings()
        .map_err(into_command_error)
}

/// Check if system is configured
#[tauri::command]
pub fn is_configured(state: State<AppState>) -> Result<bool, String> {
    let guard = state.get_db().map_err(into_command_error)?;
    let db = db_ref_or_command_error(guard.as_ref())?;

    SettingsService::new(db.executor())
        .is_configured()
        .map_err(into_command_error)
}

/// Configure as Wilaya node
#[tauri::command]
pub fn configure_as_wilaya(
    state: State<AppState>,
    wilaya_code: String,
    wilaya_name: String,
) -> Result<Settings, String> {
    let (_session, _settings) =
        authorize_command(&state, Action::AdminOnly, None).map_err(into_command_error)?;
    state.touch_session();

    let mut guard = state.get_db().map_err(into_command_error)?;
    let db = crate::commands::common::db_mut_or_command_error(guard.as_mut())?;

    let config = WilayaNodeConfiguration::new(wilaya_code, wilaya_name);

    SettingsService::new(db.executor())
        .configure_wilaya(&config)
        .map_err(into_command_error)?;

    SettingsService::new(db.executor())
        .get_settings()
        .map_err(into_command_error)
}
