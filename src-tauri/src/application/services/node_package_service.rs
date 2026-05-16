//! Node Package Service
//! Handles unit node packages parsing and importing.

use crate::errors::AppError;
use crate::models::UnitNodePackage;
use crate::repositories::{DbExecutor, RepositoryProvider};
use chrono::Utc;
use std::collections::HashMap;
use uuid::Uuid;

pub struct ParsedUnitPackageRaw {
    pub unit_id: String,
    pub unit_code: String,
    pub unit_name: String,
    pub wilaya_code: String,
    pub user_id: String,
    pub username: String,
    pub password_hash: String,
    pub role: String,
    pub node_id: String,
}

pub struct NodePackageService<'a> {
    executor: DbExecutor<'a>,
}

impl<'a> NodePackageService<'a> {
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }

    pub fn parse_unit_node_package_content(
        package_data: &str,
    ) -> Result<ParsedUnitPackageRaw, AppError> {
        let mut unit_info: HashMap<String, String> = HashMap::new();
        let mut user_info: HashMap<String, String> = HashMap::new();
        let mut current_section = "";

        for line in package_data.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with("UNIT_NODE_PACKAGE") {
                continue;
            }
            if line == "SECTION,UNIT" {
                current_section = "UNIT";
                continue;
            }
            if line == "SECTION,USER" {
                current_section = "USER";
                continue;
            }
            if let Some(stripped) = line.strip_prefix("SECTION,") {
                current_section = stripped;
                continue;
            }
            let parts: Vec<&str> = line.splitn(2, ',').collect();
            if parts.len() == 2 {
                let key = parts[0].to_string();
                let value = parts[1].trim_matches('"').to_string();
                if current_section == "UNIT" {
                    unit_info.insert(key, value);
                } else if current_section == "USER" {
                    user_info.insert(key, value);
                }
            }
        }

        let unit_id = unit_info
            .get("id")
            .ok_or_else(|| AppError::Internal("معرف الوحدة غير موجود في الحزمة".to_string()))?
            .clone();
        let unit_code = unit_info
            .get("code")
            .ok_or_else(|| AppError::Internal("كود الوحدة غير موجود في الحزمة".to_string()))?
            .clone();
        let unit_name = unit_info
            .get("name")
            .ok_or_else(|| AppError::Internal("اسم الوحدة غير موجود في الحزمة".to_string()))?
            .clone();
        let wilaya_code = unit_info
            .get("wilaya_code")
            .ok_or_else(|| AppError::Internal("كود الولاية غير موجود في الحزمة".to_string()))?
            .clone();
        let username = user_info
            .get("username")
            .ok_or_else(|| AppError::Internal("اسم المستخدم غير موجود في الحزمة".to_string()))?
            .clone();
        let password_hash = user_info
            .get("password_hash")
            .ok_or_else(|| AppError::Internal("كلمة المرور غير موجودة في الحزمة".to_string()))?
            .clone();
        let user_id = user_info
            .get("user_id")
            .ok_or_else(|| AppError::Internal("معرف المستخدم غير موجود في الحزمة".to_string()))?
            .clone();
        let role = user_info
            .get("role")
            .ok_or_else(|| AppError::Internal("دور المستخدم غير موجود في الحزمة".to_string()))?
            .clone();
        let node_id = user_info.get("node_id").cloned().unwrap_or_else(|| {
            unit_info
                .get("code")
                .cloned()
                .unwrap_or_else(|| "UNIT".to_string())
        });

        Ok(ParsedUnitPackageRaw {
            unit_id,
            unit_code,
            unit_name,
            wilaya_code,
            user_id,
            username,
            password_hash,
            role,
            node_id,
        })
    }

    pub fn import_unit_node_package(&self, package: &UnitNodePackage) -> Result<(), AppError> {
        let now = Utc::now().to_rfc3339();

        let settings_repo = self.executor.settings();
        let user_repo = self.executor.users();
        let unit_repo = self.executor.units();

        settings_repo.update_unit_node_settings(&package.unit.name, &package.unit.wilaya_code)?;

        // The password_hash was created on the WILAYA side bound to the unit code
        // (UnitService::create_unit uses `node_id = &req.code`).
        // We must store the same code here so that verify_password_argon2
        // picks up the correct node_id during login on this UNIT node.
        let node_id = &package.unit.code;

        let user_id = Uuid::new_v4().to_string();
        user_repo.upsert_raw_user(
            &user_id,
            &package.user.username,
            &package.user.password_hash,
            &package.user.role,
            node_id,
            &now,
        )?;

        unit_repo.upsert_raw_unit(
            &package.unit.id,
            &package.unit.code,
            &package.unit.name,
            &package.unit.wilaya_code,
            &now,
        )?;
        unit_repo.update_unit_user(&package.unit.id, &user_id)?;

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn import_unit_node_package_raw(
        &self,
        unit_id: &str,
        unit_code: &str,
        unit_name: &str,
        wilaya_code: &str,
        user_id: &str,
        username: &str,
        password_hash: &str,
        role_str: &str,
        node_id: &str,
    ) -> Result<(String, String), AppError> {
        let now = Utc::now().to_rfc3339();

        let unit_repo = self.executor.units();
        let user_repo = self.executor.users();
        let settings_repo = self.executor.settings();

        unit_repo.upsert_raw_unit(unit_id, unit_code, unit_name, wilaya_code, &now)?;

        let user_role = match role_str {
            "Admin" => crate::models::UserRole::Admin,
            _ => crate::models::UserRole::User,
        };

        user_repo.upsert_raw_user(
            user_id,
            username,
            password_hash,
            &user_role.to_string(),
            node_id,
            &now,
        )?;

        unit_repo.update_unit_user(unit_id, user_id)?;

        settings_repo.update_unit_node_settings(unit_name, wilaya_code)?;

        Ok((unit_id.to_string(), unit_code.to_string()))
    }
}
