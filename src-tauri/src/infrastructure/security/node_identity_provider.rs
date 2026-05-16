use crate::application::services::SettingsService;
use crate::errors::AppResult;
use crate::repositories::DbExecutor;

pub trait NodeIdentityProvider {
    fn current_node_id(&self) -> AppResult<String>;
}

pub struct SettingsNodeIdentityProvider<'a> {
    executor: DbExecutor<'a>,
}

impl<'a> SettingsNodeIdentityProvider<'a> {
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }
}

impl NodeIdentityProvider for SettingsNodeIdentityProvider<'_> {
    fn current_node_id(&self) -> AppResult<String> {
        let settings = SettingsService::new(self.executor).get_settings()?;
        Ok(settings
            .get_unit_id()
            .map(str::to_string)
            .unwrap_or_else(|| "WILAYA".to_string()))
    }
}
