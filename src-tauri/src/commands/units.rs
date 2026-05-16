//! Unit Management Commands
//!
//! Wilaya admin operations for managing units
//! Strictly follows Clean Architecture: Commands -> Services -> Repositories -> DB

use crate::application::authz::Action;
use crate::application::services::{AuditTxService, SettingsService, UnitService};
use crate::commands::common::{
    db_mut_or_command_error, db_ref_or_command_error, user_ctx_from_session,
};
use crate::commands::guards::authorize_command;
use crate::commands::types::AppState;
use crate::domain::audit::AuditAction;
use crate::errors::{into_command_error, AppError, ValidationError};
use crate::models::{CreateUnitRequest, Unit};
use tauri::State;

/// Create unit - Wilaya Admin Only
#[tauri::command]
pub fn create_unit(state: State<AppState>, request: CreateUnitRequest) -> Result<Unit, String> {
    let (session, _settings) =
        authorize_command(&state, Action::ManageUnits, None).map_err(into_command_error)?;
    state.touch_session();

    let mut guard = state.get_db().map_err(into_command_error)?;
    let db = db_mut_or_command_error(guard.as_mut())?;

    let settings = SettingsService::new(db.executor())
        .get_settings()
        .map_err(into_command_error)?;

    let wilaya_code = settings.wilaya_code.ok_or_else(|| {
        into_command_error(AppError::Validation(ValidationError::Required {
            field: "wilaya_code".to_string(),
        }))
    })?;

    let user_ctx = user_ctx_from_session(&session);

    let password_port = state.password_port.as_ref();
    let (unit, _user_id) =
        AuditTxService::execute_with_audit(db, AuditAction::CreateUnit, &user_ctx, |tx| {
            UnitService::new(tx.executor, password_port).create_unit(&request, &wilaya_code)
        })
        .map_err(into_command_error)?;

    Ok(unit)
}

/// Update unit - Wilaya Admin Only
#[tauri::command]
pub fn update_unit(
    state: State<AppState>,
    unit_id: String,
    request: CreateUnitRequest,
) -> Result<Unit, String> {
    let (session, _settings) =
        authorize_command(&state, Action::ManageUnits, None).map_err(into_command_error)?;
    state.touch_session();

    let mut guard = state.get_db().map_err(into_command_error)?;
    let db = db_mut_or_command_error(guard.as_mut())?;

    let user_ctx = user_ctx_from_session(&session);

    let password_port = state.password_port.as_ref();
    let unit = AuditTxService::execute_with_audit(db, AuditAction::UpdateUnit, &user_ctx, |tx| {
        UnitService::new(tx.executor, password_port).update_unit(&unit_id, &request)
    })
    .map_err(into_command_error)?;

    Ok(unit)
}

/// Delete unit - Wilaya Admin Only (with dependencies check)
#[tauri::command]
pub fn delete_unit(state: State<AppState>, unit_id: String) -> Result<(), String> {
    let (session, _settings) =
        authorize_command(&state, Action::ManageUnits, None).map_err(into_command_error)?;
    state.touch_session();

    let mut guard = state.get_db().map_err(into_command_error)?;
    let db = db_mut_or_command_error(guard.as_mut())?;

    let user_ctx = user_ctx_from_session(&session);

    let password_port = state.password_port.as_ref();
    AuditTxService::execute_with_audit(db, AuditAction::DeleteUnit, &user_ctx, |tx| {
        UnitService::new(tx.executor, password_port).delete_unit(&unit_id)
    })
    .map_err(into_command_error)?;

    Ok(())
}

/// Get unit by ID
#[tauri::command]
pub fn get_unit(state: State<AppState>, unit_id: String) -> Result<Option<Unit>, String> {
    // AUTHORIZATION GUARD: IDOR prevention - validate unit scope
    let (_session, _settings) =
        authorize_command(&state, Action::ReadUnits, Some(&unit_id)).map_err(into_command_error)?;
    state.touch_session();

    let guard = state.get_db().map_err(into_command_error)?;
    let db = db_ref_or_command_error(guard.as_ref())?;

    let password_port = state.password_port.as_ref();
    UnitService::new(db.executor(), password_port)
        .get_unit(&unit_id)
        .map_err(into_command_error)
}

/// List units in wilaya
#[tauri::command]
pub fn list_units(state: State<AppState>) -> Result<Vec<Unit>, String> {
    let (_session, settings) =
        authorize_command(&state, Action::ReadUnits, None).map_err(into_command_error)?;
    state.touch_session();

    let wilaya_code = settings.wilaya_code.ok_or_else(|| {
        into_command_error(AppError::Validation(ValidationError::Required {
            field: "wilaya_code".to_string(),
        }))
    })?;

    let guard = state.get_db().map_err(into_command_error)?;
    let db = db_ref_or_command_error(guard.as_ref())?;

    let password_port = state.password_port.as_ref();
    UnitService::new(db.executor(), password_port)
        .list_units(&wilaya_code)
        .map_err(into_command_error)
}
