//! Node Package Service
//! Handles unit node packages parsing and importing.

use crate::errors::{AppError, ValidationError};
use crate::models::UnitNodePackage;
use crate::repositories::{DbExecutor, RepositoryProvider};
use chrono::Utc;
use std::collections::HashMap;
use uuid::Uuid;

/// Strict role validation for `.unit` node packages (SEC-003-05-D).
///
/// Only the existing role model is accepted: `Admin` / `User`. Unknown or
/// malformed role strings are REJECTED — they are never persisted verbatim and
/// never silently mapped to a privileged role.
pub(crate) fn validate_unit_node_role(role: &str) -> Result<crate::models::UserRole, AppError> {
    match role {
        "Admin" => Ok(crate::models::UserRole::Admin),
        "User" => Ok(crate::models::UserRole::User),
        other => Err(AppError::Validation(ValidationError::InvalidFormat {
            field: "role".into(),
            message: format!("دور غير صالح في حزمة العقدة: {other}"),
        })),
    }
}

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

        // SEC-003-05-D: reject unknown/malformed roles BEFORE any write — the
        // package-controlled role string is never persisted verbatim and a
        // rejected package leaves the node untouched (fail-closed).
        let role = validate_unit_node_role(&package.user.role)?;

        // A45-08 (ADR-0045): `.unit` payloads are User-only. A `role=Admin`
        // package is not a valid bootstrap path and is rejected — the first
        // canonical Admin is established exclusively via the B8 first
        // `identity_access` import (ADR-0045).
        if role == crate::models::UserRole::Admin {
            return Err(AppError::Validation(ValidationError::InvalidFormat {
                field: "role".into(),
                message: "دور المسؤول غير صالح في حزمة العقدة (B8) — دور الحزمة يجب أن يكون User".into(),
            }));
        }

        let settings_repo = self.executor.settings();
        let user_repo = self.executor.users();
        let unit_repo = self.executor.units();

        settings_repo.update_unit_node_settings(&package.unit.name, &package.unit.wilaya_code)?;

        // The password_hash was created on the WILAYA side bound to the unit code
        // (UnitService::create_unit uses `node_id = &req.code`).
        // We must store the same code here so that verify_password_argon2
        // picks up the correct node_id during login on this UNIT node.
        let node_id = &package.unit.code;

        // ADR-0052: the operator row identity is immutable. Reuse the
        // existing `(username, node_id)` row id when present so the
        // unit→user link never dangles across re-imports.
        let user_id = user_repo
            .get_user_by_username_raw(&package.user.username, node_id)?
            .map(|u| u.id)
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        user_repo.upsert_raw_user(
            &user_id,
            &package.user.username,
            &package.user.password_hash,
            &role.to_string(),
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
            // A45-08 (ADR-0045): `.unit` payloads are User-only — Admin is
            // never a valid bootstrap role (first Admin = B8 import).
            "Admin" => {
                return Err(AppError::Validation(ValidationError::InvalidFormat {
                    field: "role".into(),
                    message: "دور المسؤول غير صالح في حزمة العقدة (B8) — دور الحزمة يجب أن يكون User"
                        .into(),
                }));
            }
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

        // ADR-0052: link by the EFFECTIVE row id — on conflict the existing
        // row keeps its own id (never rewritten).
        let effective_user_id = user_repo
            .get_user_by_username_raw(username, node_id)?
            .map(|u| u.id)
            .unwrap_or_else(|| user_id.to_string());
        unit_repo.update_unit_user(unit_id, &effective_user_id)?;

        settings_repo.update_unit_node_settings(unit_name, wilaya_code)?;

        Ok((unit_id.to_string(), unit_code.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::ConnectionFactory;
    use crate::models::{Unit, UserExport, UserRole};

    fn unit_package(role: &str) -> UnitNodePackage {
        UnitNodePackage {
            unit: Unit {
                id: "unit-a".into(),
                code: "UA".into(),
                name: "Unit A".into(),
                wilaya_code: "16".into(),
                user_id: None,
                created_at: Utc::now(),
            },
            user: UserExport {
                username: "op".into(),
                password_hash: "hash".into(),
                role: role.into(),
            },
            unit_certificate: None,
            unit_private_key: None,
        }
    }

    // ── SEC-003-05-D: strict role validation ──────────────────────────────

    #[test]
    fn valid_roles_parse() {
        assert_eq!(validate_unit_node_role("Admin").unwrap(), UserRole::Admin);
        assert_eq!(validate_unit_node_role("User").unwrap(), UserRole::User);
    }

    #[test]
    fn unknown_role_is_rejected() {
        for role in ["SuperAdmin", "admin", "OPERATOR", "", "Admin " ] {
            assert!(
                validate_unit_node_role(role).is_err(),
                "role {role:?} must be rejected"
            );
        }
    }

    #[test]
    fn import_unit_package_accepts_user_and_rejects_admin_role() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let svc = NodePackageService::new(db.executor());

        // A45-08 (ADR-0045): `.unit` payloads are User-only — Admin is not a
        // valid bootstrap role (the first canonical Admin is established via
        // the B8 `identity_access` import).
        let user_result = svc.import_unit_node_package(&unit_package("User"));
        assert!(user_result.is_ok(), "User package accepted: {user_result:?}");

        let admin_result = svc.import_unit_node_package(&unit_package("Admin"));
        assert!(
            admin_result.is_err(),
            "Admin-role .unit must be rejected (A45-08)"
        );
    }

    #[test]
    fn import_unit_package_rejects_unknown_role_before_writes() {
        let db = ConnectionFactory::new_for_test().unwrap();
        let svc = NodePackageService::new(db.executor());

        let result = svc.import_unit_node_package(&unit_package("SuperAdmin"));
        assert!(result.is_err(), "unknown-role package must be rejected");

        // Fail-closed: the settings row must not be configured by a rejected
        // package (role validation runs before any write).
        let configured = db
            .executor()
            .settings()
            .is_configured()
            .expect("settings readable");
        assert!(!configured, "rejected package must not write settings");
    }
}
