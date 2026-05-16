//! Thin per-request context for Tauri command adapters.
//!
//! Goal: build `executor` + `principal` once and pass a single handle into usecases/policies,
//! instead of threading [`crate::commands::types::AppState`] through every helper.
//! Expand gradually; commands may still use `State<AppState>` at the boundary.

use crate::application::authz::Principal;
use crate::repositories::DbExecutor;

pub struct CommandContext<'a> {
    pub executor: DbExecutor<'a>,
    pub principal: Option<Principal>,
}

impl<'a> CommandContext<'a> {
    pub fn new(executor: DbExecutor<'a>, principal: Option<Principal>) -> Self {
        Self {
            executor,
            principal,
        }
    }
}
