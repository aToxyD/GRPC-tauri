//! Shared session establishment — single path for password and challenge login.
//!
//! RFC 2026-08-04-node-identity-trust §3.7 / ADR-0038 / B3.
//!
//! Behavior-preserving extraction of the session block in the password `login`
//! command: reads settings, enforces the login authorization policy, builds the
//! `CurrentSession` (optionally reusing a challenge `session_id`), and records
//! the atomic `Login` audit + operational session begin. The caller (command or
//! integration test) stores the returned `CurrentSession` into `AppState`.

use crate::db::Database;
use crate::domain::audit::AuditAction;
use crate::domain::session::{CurrentSession, UserSnapshot};
use crate::errors::{AppError, AppResult};
use crate::models::User;

use super::{AuditTxService, LoginPolicy, OperationalSessionService, SettingsService, UserContext};

/// Result of a successful session establishment.
#[derive(Debug)]
pub struct EstablishedSession {
    pub session: CurrentSession,
    pub requires_configuration: bool,
}

/// Owns the session-establishment contract for every login path.
pub struct SessionEstablishmentService;

impl SessionEstablishmentService {
    /// Establish a session for an already-authenticated principal.
    ///
    /// `session_id_override` lets a Challenge–Response login reuse the challenge
    /// `session_id` as the session id (B3 design); the password login passes
    /// `None` and keeps the generated id.
    ///
    /// `auth_method` is telemetry persisted on the atomic `Login` audit entry as
    /// `metadata.auth_method` (`"password"` or `"challenge"`, B5). The legacy
    /// password path MUST keep tagging its logins so the deprecation window is
    /// measurable.
    ///
    /// Behavior contract (mirrors the password login command):
    /// - settings are read to compute `requires_configuration`;
    /// - `LoginPolicy::check_login_allowed` is enforced (failure aborts);
    /// - the atomic `Login` audit + `begin_session` are best-effort: on failure
    ///   the error is logged and the session is still returned (the login must
    ///   not fail because of an audit-write problem).
    pub fn establish(
        db: &mut Database,
        user: &User,
        session_id_override: Option<String>,
        auth_method: &str,
    ) -> AppResult<EstablishedSession> {
        let settings = SettingsService::new(db.executor()).get_settings()?;
        let requires_configuration = !settings.configured;

        LoginPolicy::check_login_allowed(&settings.node_type, &user.role)
            .map_err(AppError::BusinessLogic)?;

        let snapshot = UserSnapshot {
            id: user.id.clone(),
            username: user.username.clone(),
            role: user.role.clone(),
            created_at: user.created_at,
        };
        let session = CurrentSession::new_with_session_id(
            session_id_override,
            user.id.clone(),
            user.username.clone(),
            user.role.clone(),
            snapshot,
        );
        let session_id = session.session_id.clone();

        let user_ctx = UserContext::new(&user.id, &user.username, Some(&session_id));
        if let Err(e) = AuditTxService::execute_with_audit_metadata(
            db,
            AuditAction::Login,
            &user_ctx,
            Some(serde_json::json!({ "auth_method": auth_method })),
            |tx| {
                OperationalSessionService::new(tx.executor).begin_session(
                    &session_id,
                    &user.id,
                    &user.username,
                )
            },
        ) {
            log::error!(
                target: "grpc::audit",
                "AUDIT WRITE FAILED [session_establishment] user={} err={:?}",
                user.username,
                e
            );
        }

        Ok(EstablishedSession {
            session,
            requires_configuration,
        })
    }
}
