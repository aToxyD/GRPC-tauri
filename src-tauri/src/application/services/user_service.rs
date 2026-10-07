use crate::errors::{AppError, AuthenticationError, ValidationError};
use crate::infrastructure::security::{NodeIdentityProvider, SettingsNodeIdentityProvider};
use crate::models::UserRole;
use crate::repositories::{DbExecutor, RepositoryProvider};
use chrono::Utc;
use uuid::Uuid;

use crate::application::services::{unit_service::OPERATOR_USERNAME, SettingsService};
use crate::domain::security::PasswordHashPort;

pub struct UserService<'a> {
    executor: DbExecutor<'a>,
    password_port: &'a dyn PasswordHashPort,
}

impl<'a> UserService<'a> {
    pub fn new(executor: DbExecutor<'a>, password_port: &'a dyn PasswordHashPort) -> Self {
        Self {
            executor,
            password_port,
        }
    }

    pub fn create_user(
        &self,
        username: &str,
        password: &str,
        role: UserRole,
    ) -> Result<String, AppError> {
        crate::domain::validation::validate_change_password(password)?;

        let node_id = SettingsNodeIdentityProvider::new(self.executor).current_node_id()?;

        let password_hash = self
            .password_port
            .hash_node(password, &node_id)
            .map_err(crate::errors::AppError::Internal)?;

        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        self.executor
            .users()
            // Not the canonical UNIT bootstrap path (ADR-0063 §4): this generic
            // account-creation flow always starts outside the forced state.
            .upsert_user(&id, username, &password_hash, role, &node_id, false, &now)?;
        Ok(id)
    }

    pub fn get_user_by_username(
        &self,
        username: &str,
        node_scope: &str,
    ) -> Result<Option<crate::models::User>, AppError> {
        self.executor
            .users()
            .get_user_by_username(username, node_scope)
    }

    pub fn get_user_by_id(&self, id: &str) -> Result<Option<crate::models::User>, AppError> {
        self.executor.users().get_user_by_id(id)
    }

    /// ADR-0063 §6 — canonical local UNIT operator self password change.
    ///
    /// `user_id` is always the authenticated session's own id, supplied by the
    /// command; there is no caller-selected target. Authorization (UNIT node +
    /// canonical operator identity + `User` role) is owned exclusively by
    /// `application::authz` — this service performs credential mechanics only,
    /// so the rule exists in exactly one place.
    ///
    /// Fail-closed at every step, and deliberately free of any rate-limiter or
    /// session interaction (§6.7, §6.9): the login buckets are untouched and the
    /// established session is neither invalidated nor replaced.
    pub fn change_own_password(
        &self,
        user_id: &str,
        current_password: &str,
        new_password: &str,
    ) -> Result<(), AppError> {
        let user = self
            .executor
            .users()
            .get_user_by_id(user_id)?
            .ok_or_else(|| {
                AppError::Internal("password change target does not exist".to_string())
            })?;

        // §6.2 — verify the current password with the node-bound verifier. A
        // mismatch yields a single non-enumerating error carrying no username,
        // no user id, and no account-existence signal.
        let current_ok = self
            .password_port
            .verify_node(current_password, &user.node_id, &user.password_hash)
            .map_err(AppError::Internal)?;
        if !current_ok {
            return Err(AppError::Authentication(
                AuthenticationError::InvalidCurrentPassword,
            ));
        }

        // §6.3 — the existing password policy, unchanged.
        crate::domain::validation::validate_change_password(new_password)?;

        // §6.4 — reuse rejection: the proposed password is verified against the
        // stored node-bound hash. Equality of hashes implies equality of the
        // plaintexts, so this decides reuse without ever comparing plaintext.
        let is_reuse = self
            .password_port
            .verify_node(new_password, &user.node_id, &user.password_hash)
            .map_err(AppError::Internal)?;
        if is_reuse {
            return Err(AppError::Validation(ValidationError::PasswordReuse {
                reason: "the new password must differ from the current one".to_string(),
            }));
        }

        // §6.5 — node-bound hash of the new credential (Argon2, unchanged).
        let new_hash = self
            .password_port
            .hash_node(new_password, &user.node_id)
            .map_err(AppError::Internal)?;

        // §6.6 — password write + forced-state clear as one fail-closed unit on
        // the caller's transaction executor.
        let now = Utc::now().to_rfc3339();
        self.executor
            .users()
            .change_password_and_clear_forced_state(user_id, &new_hash, &now)
    }

    /// ADR-0063 §7.1 — local UNIT admin reset of the canonical local `user`.
    ///
    /// Authorization (UNIT node + canonical `admin`) is owned exclusively by
    /// `application::authz` — this service performs credential mechanics only,
    /// so the rule exists in exactly one place.
    ///
    /// The target row is derived **server-side** from the authoritative local
    /// UNIT code (SEC-029: `units.code == users.node_id == Settings.unit_code`):
    /// no username or unit id is caller-supplied, so a caller can never select
    /// or redirect the target. A non-UNIT / unconfigured node resolves to no
    /// usable scope and fails closed.
    ///
    /// Fail-closed at every step, and deliberately free of any rate-limiter or
    /// session interaction (§7.1 F29, F30, F32): the login buckets are
    /// untouched and the authenticated admin's session is neither invalidated
    /// nor replaced.
    pub fn reset_local_operator_password(&self, temporary_password: &str) -> Result<(), AppError> {
        let settings = SettingsService::new(self.executor).get_settings()?;
        let local_unit_code = settings
            .get_unit_id()
            .ok_or_else(|| {
                AppError::Internal(
                    "operator reset requires a configured local UNIT node".to_string(),
                )
            })?
            .to_string();

        // Active only (deleted = 0): a revoked operator has no reset target,
        // and this operation must never re-enable one.
        let operator = self
            .executor
            .users()
            .get_user_by_username(OPERATOR_USERNAME, &local_unit_code)?
            .ok_or_else(|| {
                AppError::Internal("operator reset target does not exist".to_string())
            })?;

        // §7.1 — the temporary credential must satisfy the normal policy (F7).
        // Literal `0000` (and any other weak shape) is rejected here.
        crate::domain::validation::validate_change_password(temporary_password)?;

        // §7.1 — node-bound hash of the temporary credential (Argon2,
        // unchanged): `hash_node(temp, <local unit code>)` (F2, F4).
        let new_hash = self
            .password_port
            .hash_node(temporary_password, &local_unit_code)
            .map_err(crate::errors::AppError::Internal)?;

        // §7.1 — password write + forced-state set as one fail-closed unit on
        // the caller's transaction executor.
        let now = Utc::now().to_rfc3339();
        self.executor
            .users()
            .change_password_and_set_forced_state(&operator.id, &new_hash, &now)
    }
}
