use crate::application::authz::Action;
use crate::application::services::{IntegrityReport, IntegrityService};
use crate::commands::common::db_ref_or_command_error;
use crate::commands::guards::authorize_command;
use crate::commands::types::AppState;
use crate::errors::into_command_error;
use tauri::State;

#[tauri::command]
pub fn verify_integrity(state: State<AppState>) -> Result<IntegrityReport, String> {
    let (_session, _settings) =
        authorize_command(&state, Action::ViewSystemHealth, None).map_err(into_command_error)?;
    state.touch_session();
    let guard = state.get_db().map_err(into_command_error)?;
    let db = db_ref_or_command_error(guard.as_ref())?;
    IntegrityService::new(db.executor())
        .run()
        .map_err(into_command_error)
}
