//! Deterministic operational session reconstruction — append-only metadata, no live presence.

use crate::errors::AppError;
use crate::repositories::executor::DbExecutor;
use chrono::Utc;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SessionEndReason {
    Logout,
    UnexpectedTermination,
}

impl SessionEndReason {
    fn as_db_str(self) -> &'static str {
        match self {
            Self::Logout => "LOGOUT",
            Self::UnexpectedTermination => "UNEXPECTED_TERMINATION",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationalSessionRecord {
    pub id: String,
    pub user_id: String,
    pub username: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub end_reason: Option<String>,
    pub critical_operations_count: i64,
    pub integrity_warnings_count: i64,
    pub anomalies_surfaced_count: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionCounterKind {
    CriticalOperation,
    IntegrityWarning,
    AnomalySurfaced,
}

pub struct OperationalSessionService<'a> {
    executor: DbExecutor<'a>,
}

impl<'a> OperationalSessionService<'a> {
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }

    /// Called once at startup before normal operations — marks dangling sessions.
    pub fn recover_abandoned_sessions(&self) -> Result<u64, AppError> {
        let now = Utc::now().to_rfc3339();
        use crate::repositories::RepositoryProvider;
        let updated = self.executor.sessions().recover_abandoned(&now)?;
        if updated > 0 {
            log::warn!(
                target: "grpc::session",
                "[SESSION_RECOVERY] closed {} session(s) as UNEXPECTED_TERMINATION",
                updated
            );
        }
        Ok(updated)
    }

    pub fn begin_session(
        &self,
        session_id: &str,
        user_id: &str,
        username: &str,
    ) -> Result<(), AppError> {
        let started_at = Utc::now().to_rfc3339();
        use crate::repositories::RepositoryProvider;
        self.executor
            .sessions()
            .insert_session(session_id, user_id, username, &started_at)?;
        log::info!(
            target: "grpc::session",
            "[SESSION_START] id={} user={}",
            session_id,
            username
        );
        Ok(())
    }

    pub fn end_session(&self, session_id: &str, reason: SessionEndReason) -> Result<(), AppError> {
        let ended_at = Utc::now().to_rfc3339();
        use crate::repositories::RepositoryProvider;
        let updated = self.executor.sessions().update_session_end(
            session_id,
            &ended_at,
            reason.as_db_str(),
        )?;
        if updated == 0 {
            log::warn!(
                target: "grpc::session",
                "[SESSION_END_SKIPPED] id={} reason={:?} (already closed or missing)",
                session_id,
                reason
            );
        } else {
            log::info!(
                target: "grpc::session",
                "[SESSION_END] id={} reason={:?}",
                session_id,
                reason
            );
        }
        Ok(())
    }

    pub fn increment_counter(
        &self,
        session_id: &str,
        kind: SessionCounterKind,
    ) -> Result<(), AppError> {
        let column = match kind {
            SessionCounterKind::CriticalOperation => "critical_operations_count",
            SessionCounterKind::IntegrityWarning => "integrity_warnings_count",
            SessionCounterKind::AnomalySurfaced => "anomalies_surfaced_count",
        };
        use crate::repositories::RepositoryProvider;
        self.executor
            .sessions()
            .increment_counter(session_id, column)?;
        Ok(())
    }

    pub fn get_session(
        &self,
        session_id: &str,
    ) -> Result<Option<OperationalSessionRecord>, AppError> {
        use crate::repositories::RepositoryProvider;
        let row = self.executor.sessions().get_by_id(session_id)?;
        Ok(row.map(|r| OperationalSessionRecord {
            id: r.id,
            user_id: r.user_id,
            username: r.username,
            started_at: r.started_at,
            ended_at: r.ended_at,
            end_reason: r.end_reason,
            critical_operations_count: r.critical_operations_count,
            integrity_warnings_count: r.integrity_warnings_count,
            anomalies_surfaced_count: r.anomalies_surfaced_count,
        }))
    }

    pub fn list_recent(&self, limit: i64) -> Result<Vec<OperationalSessionRecord>, AppError> {
        use crate::repositories::RepositoryProvider;
        let rows = self.executor.sessions().list_recent(limit)?;
        Ok(rows
            .into_iter()
            .map(|r| OperationalSessionRecord {
                id: r.id,
                user_id: r.user_id,
                username: r.username,
                started_at: r.started_at,
                ended_at: r.ended_at,
                end_reason: r.end_reason,
                critical_operations_count: r.critical_operations_count,
                integrity_warnings_count: r.integrity_warnings_count,
                anomalies_surfaced_count: r.anomalies_surfaced_count,
            })
            .collect())
    }
}

impl crate::architecture::Service for OperationalSessionService<'_> {}
