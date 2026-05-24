use crate::errors::AppError;
use crate::errors::BusinessLogicError;
use crate::models::{NodeType, Settings};
use crate::repositories::{DbExecutor, RepositoryProvider};

pub struct SettingsService<'a> {
    executor: DbExecutor<'a>,
}

impl<'a> SettingsService<'a> {
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }

    pub fn get_settings(&self) -> Result<Settings, AppError> {
        let repo = self.executor.settings();
        let mut settings = repo.get_settings_row()?;

        if settings.node_type == NodeType::Unit {
            settings.unit_code = repo.get_first_unit_code()?;
        }

        Ok(settings)
    }

    pub fn is_configured(&self) -> Result<bool, AppError> {
        let settings = self.get_settings()?;

        if !settings.configured {
            return Ok(false);
        }

        match settings.node_type {
            NodeType::Unit => Ok(settings.unit_name.is_some() && settings.unit_code.is_some()),
            NodeType::Wilaya => {
                Ok(settings.wilaya_code.is_some() && settings.wilaya_name.is_some())
            }
        }
    }

    pub fn get_node_type(&self) -> Result<NodeType, AppError> {
        let settings = self.get_settings()?;

        // Enforce readiness rules for configured systems.
        if settings.configured {
            match settings.node_type {
                NodeType::Unit => {
                    if settings.unit_name.is_none() || settings.unit_code.is_none() {
                        return Err(AppError::BusinessLogic(
                            BusinessLogicError::UnitNotConfigured,
                        ));
                    }
                }
                NodeType::Wilaya => {
                    if settings.wilaya_code.is_none() || settings.wilaya_name.is_none() {
                        return Err(AppError::BusinessLogic(
                            BusinessLogicError::UnitNotConfigured,
                        ));
                    }
                }
            }
        }

        Ok(settings.node_type)
    }

    pub fn is_setup_mode(&self) -> Result<bool, AppError> {
        Ok(!self.get_settings()?.configured)
    }

    pub fn get_current_unit_id(&self) -> Result<Option<String>, AppError> {
        let settings = self.get_settings()?;
        if settings.node_type != NodeType::Unit {
            return Ok(None);
        }

        // Resolve canonical units.id from settings.unit_name and wilaya_code
        let unit_name = settings.unit_name.as_deref();
        let wilaya_code = settings.wilaya_code.as_deref();

        if let (Some(name), Some(wc)) = (unit_name, wilaya_code) {
            match self
                .executor
                .units()
                .find_unit_id_by_name_and_wilaya(name, wc)?
            {
                Some(id) => {
                    log::info!(
                        target: "grpc::settings",
                        "Resolved canonical unit_id={} for unit_name={} wilaya={}",
                        id, name, wc
                    );
                    Ok(Some(id))
                }
                None => {
                    // Fallback to unit_name if not found in units table
                    log::warn!(
                        target: "grpc::settings",
                        "Unit not found in units table for name={} wilaya={}, falling back to unit_name as unit_id",
                        name, wc
                    );
                    Ok(settings.unit_name.clone())
                }
            }
        } else {
            Ok(settings.unit_name.clone())
        }
    }

    pub fn configure_wilaya(
        &self,
        config: &crate::models::WilayaNodeConfiguration,
    ) -> Result<(), AppError> {
        config.validate().map_err(AppError::Internal)?;
        self.executor.settings().configure_wilaya(config)
    }

    pub fn update_unit_node_settings(
        &self,
        unit_name: &str,
        wilaya_code: &str,
    ) -> Result<(), AppError> {
        if unit_name.trim().is_empty() || wilaya_code.trim().is_empty() {
            return Err(AppError::Internal(
                "unit_name and wilaya_code are required".to_string(),
            ));
        }
        self.executor
            .settings()
            .update_unit_node_settings(unit_name, wilaya_code)
    }

    pub fn set_current_year(&self, year: i32) -> Result<(), AppError> {
        if !(2000..=2100).contains(&year) {
            return Err(AppError::Internal(
                "current year out of allowed range".to_string(),
            ));
        }
        self.executor.settings().set_current_year(year)
    }

    pub fn get_sync_preflight_check(&self) -> Result<crate::models::SyncPreflightCheck, AppError> {
        let settings = self.get_settings()?;
        let is_configured = settings.configured;
        let node_id = settings.unit_name.clone().or(settings.wilaya_name.clone());

        let mut reasons = Vec::new();
        let mut reason_messages_ar = Vec::new();

        if !is_configured {
            reasons.push("not_configured".to_string());
            reason_messages_ar.push("النظام غير مهيأ بعد".to_string());
        }
        if node_id.is_none() {
            reasons.push("missing_node_id".to_string());
            reason_messages_ar.push("معرف العقدة مفقود".to_string());
        }

        let status = if reasons.is_empty() {
            "ok".to_string()
        } else {
            "fail".to_string()
        };

        let diagnostics = crate::infrastructure::security::get_sync_security_diagnostics()?;

        Ok(crate::models::SyncPreflightCheck {
            status,
            reasons,
            reason_messages_ar,
            diagnostics,
        })
    }
}
