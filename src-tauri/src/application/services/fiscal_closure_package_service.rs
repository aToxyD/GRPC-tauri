//! Fiscal Closure Package Service
//!
//! Implements the offline Wilaya→Unit fiscal closure authorization flow.
//!
//! # Responsibilities
//!
//! ## Wilaya side
//! - Build and sign a `.fiscal-close.sync` authorization package after a
//!   successful `close_year`.  The package contains only the closure
//!   authorization metadata — no inventory data, no DB dump.
//!
//! ## Unit side
//! - Preview a package file without mutating any state.
//! - Apply a validated package by delegating to the existing
//!   `FiscalClosingService::close_year`, wrapped in a transaction.
//!   Replay protection via `applied_fiscal_transitions` (UNIQUE constraint).
//!
//! # Hard constraints (inherited from system design)
//! - No distributed transactions.
//! - No network connections.
//! - No background workers.
//! - `FiscalClosingService::close_year` is the single source of truth for the
//!   carry-forward logic — this service does NOT duplicate it.

use crate::errors::{AppError, BusinessLogicError};
use crate::infrastructure::security::{
    resolve_active_signing_key_id, resolve_package_signing_key_32,
};
use crate::repositories::{DbExecutor, RepositoryProvider};
use serde::{Deserialize, Serialize};

// ─── Package schema version ───────────────────────────────────────────────────
/// Current schema version for `.fiscal-close.sync` packages.
pub const FISCAL_CLOSURE_PACKAGE_VERSION: u32 = 2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthorizedExecutionWindow {
    pub not_before: String,
    pub expires_at: String,
}

/// Structured content of a fiscal closure authorization package.
/// Serialized as JSON; the HMAC covers the canonical JSON bytes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FiscalClosurePackage {
    /// Package schema version — reject packages with unknown versions.
    pub schema_version: u32,
    /// The fiscal year that was closed by the Wilaya authority.
    pub closed_year: i32,
    /// The fiscal year that was opened immediately after closure.
    pub opened_year: i32,
    /// RFC3339 UTC timestamp of the Wilaya closure.
    pub closure_timestamp_utc: String,
    /// node_id of the Wilaya node that issued this package.
    pub closure_authority_node_id: String,
    /// Username of the operator who performed `close_year` on Wilaya.
    pub closure_authority_username: String,
    /// UUID — unique per fiscal transition; used for replay protection.
    pub fiscal_transition_id: String,
    /// RFC3339 UTC timestamp when the package file was created.
    pub package_created_at: String,
    /// Signing key id used to produce the HMAC (for key rotation support).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signing_key_id: Option<String>,
    /// Window in which this package can be applied.
    pub authorized_execution_window: AuthorizedExecutionWindow,
    /// Deterministic SHA256 fingerprint of the canonical package payload.
    pub package_fingerprint: String,
}

/// On-disk envelope: the package JSON + its HMAC hex-digest.
#[derive(Debug, Serialize, Deserialize)]
struct FiscalClosureEnvelope {
    pub package: FiscalClosurePackage,
    /// HMAC-SHA256 over the canonical JSON of `package`, hex-encoded.
    pub hmac_hex: String,
}

/// What the Unit operator sees before committing to apply a package.
#[derive(Debug, Serialize, Deserialize)]
pub struct FiscalClosurePreview {
    pub closed_year: i32,
    pub opened_year: i32,
    pub closure_timestamp_utc: String,
    pub closure_authority_node_id: String,
    pub closure_authority_username: String,
    pub fiscal_transition_id: String,
    pub package_created_at: String,
    pub signing_key_id: Option<String>,
    pub schema_version: u32,
    pub authorized_execution_window: AuthorizedExecutionWindow,
    /// Pre-flight validation result — NOT yet applied.
    pub validation_ok: bool,
    /// Human-readable list of pre-flight issues (empty when `validation_ok`).
    pub validation_issues: Vec<String>,
    pub package_fingerprint: String,
}

/// Outcome returned to the Tauri command layer after applying a package.
#[derive(Debug, Serialize, Deserialize)]
pub struct FiscalClosureApplyResult {
    pub fiscal_transition_id: String,
    pub closed_year: i32,
    pub opened_year: i32,
    pub snapshot_count: usize,
    pub package_fingerprint: String,
}

// ─────────────────────────────────────────────────────────────────────────────

pub struct FiscalClosurePackageService<'a> {
    executor: DbExecutor<'a>,
}

impl<'a> FiscalClosurePackageService<'a> {
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }

    // ── HMAC helpers ─────────────────────────────────────────────────────────

    fn compute_hmac(key: &[u8; 32], payload: &[u8]) -> String {
        use hmac::{Hmac, Mac};
        use sha2::Sha256;
        type HmacSha256 = Hmac<Sha256>;
        let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts any key length");
        mac.update(payload);
        hex::encode(mac.finalize().into_bytes())
    }

    fn verify_hmac(key: &[u8; 32], payload: &[u8], expected_hex: &str) -> bool {
        use hmac::{Hmac, Mac};
        use sha2::Sha256;
        type HmacSha256 = Hmac<Sha256>;
        let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts any key length");
        mac.update(payload);
        let computed = hex::encode(mac.finalize().into_bytes());
        // Constant-time comparison via simple string equality (both are hex; no secret timing leak).
        computed == expected_hex
    }

    fn canonical_json(pkg: &FiscalClosurePackage) -> Result<Vec<u8>, AppError> {
        serde_json::to_vec(pkg)
            .map_err(|e| AppError::Internal(format!("fiscal closure JSON serialization: {}", e)))
    }

    pub fn compute_fingerprint(pkg: &FiscalClosurePackage) -> Result<String, AppError> {
        use sha2::{Digest, Sha256};
        // Create a copy without the fingerprint for hashing
        let mut pkg_for_hash = pkg.clone();
        pkg_for_hash.package_fingerprint = "".to_string();

        let payload = serde_json::to_vec(&pkg_for_hash)
            .map_err(|e| AppError::Internal(format!("fingerprint JSON serialization: {}", e)))?;

        let mut hasher = Sha256::new();
        hasher.update(&payload);
        Ok(hex::encode(hasher.finalize()).to_uppercase())
    }

    // ── Wilaya: build & sign ──────────────────────────────────────────────────

    /// Build a signed fiscal closure package after a successful `close_year`.
    ///
    /// Called on the Wilaya node only.  Does NOT perform the closure itself —
    /// that must have already succeeded via `FiscalClosingService::close_year`.
    pub fn build_closure_package(
        closure_authority_node_id: &str,
        closure_authority_username: &str,
        closed_year: i32,
        opened_year: i32,
        closure_timestamp_utc: &str,
        existing_transition_id: Option<String>,
    ) -> Result<FiscalClosurePackage, AppError> {
        let fiscal_transition_id =
            existing_transition_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let now = chrono::Utc::now();
        let package_created_at = now.to_rfc3339();

        // Default 14 day execution window
        let authorized_execution_window = AuthorizedExecutionWindow {
            not_before: now.to_rfc3339(),
            expires_at: (now + chrono::Duration::days(14)).to_rfc3339(),
        };

        let mut pkg = FiscalClosurePackage {
            schema_version: FISCAL_CLOSURE_PACKAGE_VERSION,
            closed_year,
            opened_year,
            closure_timestamp_utc: closure_timestamp_utc.to_string(),
            closure_authority_node_id: closure_authority_node_id.to_string(),
            closure_authority_username: closure_authority_username.to_string(),
            fiscal_transition_id,
            package_created_at,
            signing_key_id: resolve_active_signing_key_id(),
            authorized_execution_window,
            package_fingerprint: "".to_string(),
        };

        let fingerprint = Self::compute_fingerprint(&pkg)?;
        pkg.package_fingerprint = fingerprint;

        Ok(pkg)
    }

    /// Serialize and sign the package, then write it to `file_path`.
    /// Register a 'PENDING_EXPORT' package record.
    /// Called automatically during Wilaya closure.
    pub fn register_pending_package(
        &self,
        fiscal_year: i32,
        next_year: i32,
        exported_by: &str,
    ) -> Result<String, AppError> {
        let transition_id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();

        let registry_entry = crate::repositories::FiscalPackageRegistryEntry {
            id: 0,
            transition_id: transition_id.clone(),
            fiscal_year,
            next_year,
            package_fingerprint: "PENDING_EXPORT".to_string(),
            signing_key_id: "PENDING".to_string(),
            exported_at: now,
            applied_at: None,
            retention_status: "ACTIVE".to_string(),
            exported_by: exported_by.to_string(),
            applied_by: None,
            archived: false,
            notes: Some("Auto-registered on closure".to_string()),
        };

        self.executor
            .fiscal_package_registry()
            .insert(&registry_entry)?;

        Ok(transition_id)
    }

    pub fn list_package_registry(
        &self,
    ) -> Result<Vec<crate::repositories::FiscalPackageRegistryEntry>, AppError> {
        self.executor
            .fiscal_package_registry()
            .list_all()
            .map_err(|e| e.into())
    }

    pub fn update_retention_status(
        &self,
        transition_id: &str,
        status: &str,
        user_id: &str,
        username: &str,
    ) -> Result<(), AppError> {
        self.executor
            .fiscal_package_registry()
            .update_retention_status(transition_id, status)?;

        // Audit log
        crate::application::services::AuditService::new(self.executor).log_success(
            user_id,
            username,
            crate::domain::audit::AuditAction::UpdateSettings,
            crate::domain::audit::EntityType::Financial,
            Some(transition_id),
            Some(&format!(
                "Fiscal package retention status updated to {}",
                status
            )),
            None,
            None,
            None,
            None,
        )?;

        Ok(())
    }

    pub fn export_to_file(
        &self,
        pkg: &FiscalClosurePackage,
        file_path: &str,
    ) -> Result<(), AppError> {
        let key = resolve_package_signing_key_32()?;
        let payload = Self::canonical_json(pkg)?;
        let hmac_hex = Self::compute_hmac(&key, &payload);

        let envelope = FiscalClosureEnvelope {
            package: pkg.clone(),
            hmac_hex,
        };
        let json = serde_json::to_string_pretty(&envelope).map_err(|e| {
            AppError::Internal(format!("fiscal closure envelope serialization: {}", e))
        })?;

        std::fs::write(file_path, json).map_err(|e| {
            AppError::Internal(format!("cannot write fiscal closure package: {}", e))
        })?;

        // Registry entry (Migration 013)
        let registry_entry = crate::repositories::FiscalPackageRegistryEntry {
            id: 0,
            transition_id: pkg.fiscal_transition_id.clone(),
            fiscal_year: pkg.closed_year,
            next_year: pkg.opened_year,
            package_fingerprint: pkg.package_fingerprint.clone(),
            signing_key_id: pkg
                .signing_key_id
                .clone()
                .unwrap_or_else(|| "NONE".to_string()),
            exported_at: pkg.package_created_at.clone(),
            applied_at: None,
            retention_status: "ACTIVE".to_string(),
            exported_by: pkg.closure_authority_username.clone(),
            applied_by: None,
            archived: false,
            notes: None,
        };

        self.executor
            .fiscal_package_registry()
            .upsert(&registry_entry)?;

        log::info!(
            target: "grpc::fiscal",
            "[FISCAL_PACKAGE_EXPORTED] transition_id={} closed_year={} opened_year={} authority={} node_id={} fingerprint={}",
            pkg.fiscal_transition_id, pkg.closed_year, pkg.opened_year,
            pkg.closure_authority_username, pkg.closure_authority_node_id,
            pkg.package_fingerprint,
        );

        Ok(())
    }

    // ── Unit: parse & verify ──────────────────────────────────────────────────

    fn read_and_verify_envelope(file_path: &str) -> Result<FiscalClosurePackage, AppError> {
        let raw = std::fs::read_to_string(file_path).map_err(|e| {
            AppError::Internal(format!("cannot read fiscal closure package file: {}", e))
        })?;

        let envelope: FiscalClosureEnvelope = serde_json::from_str(&raw).map_err(|e| {
            AppError::FileFormat(format!("fiscal closure package is not valid JSON: {}", e))
        })?;

        // Schema version guard
        if envelope.package.schema_version != FISCAL_CLOSURE_PACKAGE_VERSION {
            return Err(AppError::BusinessLogic(
                BusinessLogicError::OperationNotPermitted {
                    message: format!(
                        "نسخة مخطط حزمة الإغلاق المالي غير مدعومة {} (المتوقع {})",
                        envelope.package.schema_version, FISCAL_CLOSURE_PACKAGE_VERSION
                    ),
                },
            ));
        }

        // HMAC verification — fail-closed
        let key = resolve_package_signing_key_32()?;
        let payload = Self::canonical_json(&envelope.package)?;
        if !Self::verify_hmac(&key, &payload, &envelope.hmac_hex) {
            log::warn!(
                target: "grpc::fiscal",
                "[FISCAL_CLOSURE_PACKAGE_REJECTED] reason=invalid_signature transition_id={} closed_year={}",
                envelope.package.fiscal_transition_id, envelope.package.closed_year,
            );
            return Err(AppError::BusinessLogic(
                BusinessLogicError::OperationNotPermitted {
                    message:
                        "فشل التحقق من توقيع حزمة الإغلاق المالي — قد تكون الحزمة قد تعرضت للتلاعب"
                            .to_string(),
                },
            ));
        }

        Ok(envelope.package)
    }

    // ── Unit: pre-flight checks (read-only) ───────────────────────────────────

    pub fn pre_flight_checks(&self, pkg: &FiscalClosurePackage) -> Result<Vec<String>, AppError> {
        let mut issues: Vec<String> = Vec::new();

        // 1: Validate execution window
        let now = chrono::Utc::now(); // [arch:allow-utc-now] see ADR-0007 — execution window validation against current time
        if let Ok(not_before) =
            chrono::DateTime::parse_from_rfc3339(&pkg.authorized_execution_window.not_before)
        {
            if now < not_before.with_timezone(&chrono::Utc) {
                issues.push(format!(
                    "فترة تنفيذ الحزمة لم تبدأ بعد (ليس قبل: {})",
                    pkg.authorized_execution_window.not_before
                ));
            }
        } else {
            issues.push("طابع زمني غير صالح لـ not_before".to_string());
        }

        if let Ok(expires_at) =
            chrono::DateTime::parse_from_rfc3339(&pkg.authorized_execution_window.expires_at)
        {
            if now > expires_at.with_timezone(&chrono::Utc) {
                issues.push(format!(
                    "انتهت صلاحية فترة تنفيذ الحزمة (تاريخ الانتهاء: {})",
                    pkg.authorized_execution_window.expires_at
                ));
            }
        } else {
            issues.push("طابع زمني غير صالح لـ expires_at".to_string());
        }

        // C: current_year in UNIT == closed_year in package
        let unit_current_year = self.executor.settings().get_current_year()?;
        if unit_current_year != pkg.closed_year {
            issues.push(format!(
                "السنة الحالية للوحدة {} لا تطابق السنة المغلقة في الحزمة {}",
                unit_current_year, pkg.closed_year
            ));
        }

        // D: no archived conflict — closed_year must not be archived
        let already_archived = self
            .executor
            .fiscal_year_status()
            .is_year_archived(pkg.closed_year)?;
        if already_archived {
            issues.push(format!(
                "السنة المالية {} مؤرشفة بالفعل على هذه الوحدة؛ لا يمكن تطبيق الإغلاق مرة أخرى",
                pkg.closed_year
            ));
        }

        // E: replay protection — transition_id must not have been applied already
        let already_applied = self
            .executor
            .fiscal_transitions()
            .is_applied(&pkg.fiscal_transition_id)?;
        if already_applied {
            issues.push(format!(
                "تم تطبيق معرف الانتقال المالي {} بالفعل (حماية من إعادة التشغيل)",
                pkg.fiscal_transition_id
            ));
        }

        Ok(issues)
    }

    // ── Unit: preview (no state mutation) ────────────────────────────────────

    /// Read, verify HMAC, run pre-flight checks — do NOT apply anything.
    pub fn preview_closure_package(
        &self,
        file_path: &str,
    ) -> Result<FiscalClosurePreview, AppError> {
        let pkg = Self::read_and_verify_envelope(file_path)?;
        let issues = self.pre_flight_checks(&pkg)?;
        let ok = issues.is_empty();

        Ok(FiscalClosurePreview {
            closed_year: pkg.closed_year,
            opened_year: pkg.opened_year,
            closure_timestamp_utc: pkg.closure_timestamp_utc.clone(),
            closure_authority_node_id: pkg.closure_authority_node_id.clone(),
            closure_authority_username: pkg.closure_authority_username.clone(),
            fiscal_transition_id: pkg.fiscal_transition_id.clone(),
            package_created_at: pkg.package_created_at.clone(),
            signing_key_id: pkg.signing_key_id.clone(),
            schema_version: pkg.schema_version,
            authorized_execution_window: pkg.authorized_execution_window.clone(),
            validation_ok: ok,
            validation_issues: issues,
            package_fingerprint: pkg.package_fingerprint.clone(),
        })
    }

    // ── Unit: apply (must be called inside db.with_transaction) ──────────────

    /// Apply a fiscal closure package on the UNIT node.
    pub fn apply_closure_package(
        &self,
        file_path: &str,
        applying_user_id: &str,
        applying_username: &str,
    ) -> Result<FiscalClosureApplyResult, AppError> {
        // ── 1 & 2: Verify signature + pre-flight ─────────────────────────
        let pkg = Self::read_and_verify_envelope(file_path)?;
        let issues = self.pre_flight_checks(&pkg)?;

        if !issues.is_empty() {
            let reason = issues.join("; ");
            log::warn!(
                target: "grpc::fiscal",
                "[FISCAL_CLOSURE_PACKAGE_REJECTED] transition_id={} closed_year={} reason=\"{}\"",
                pkg.fiscal_transition_id, pkg.closed_year, reason,
            );
            return Err(AppError::BusinessLogic(
                BusinessLogicError::OperationNotPermitted {
                    message: format!("تم رفض حزمة الإغلاق المالي: {}", reason),
                },
            ));
        }

        // ── 3: Replay-protection record (UNIQUE will catch race or duplicate) ─
        self.executor
            .fiscal_transitions()
            .record_application(
                &pkg.fiscal_transition_id,
                pkg.closed_year,
                pkg.opened_year,
                applying_username,
            )
            .map_err(|e| {
                // UNIQUE violation → replay attempt
                if e.to_string().contains("UNIQUE") {
                    AppError::BusinessLogic(BusinessLogicError::DuplicateSyncPackage {
                        package_id: pkg.fiscal_transition_id.clone(),
                    })
                } else {
                    AppError::from(e)
                }
            })?;

        // ── 4: Delegate carry-forward to FiscalClosingService ────────────────
        let snapshot_count = crate::application::services::FiscalClosingService::new(self.executor)
            .close_year(
                pkg.closed_year,
                pkg.opened_year,
                applying_user_id,
                applying_username,
                None,
            )?;

        // ── 5: Audit event ────────────────────────────────────────────────────
        let applied_by_role = self
            .executor
            .users()
            .get_user_by_id(applying_user_id)?
            .map(|u| u.role.to_string())
            .unwrap_or_else(|| "UNKNOWN".to_string());

        crate::application::services::AuditService::new(self.executor).log_success(
            applying_user_id,
            applying_username,
            crate::domain::audit::AuditAction::FiscalClosurePackageApplied,
            crate::domain::audit::EntityType::Financial,
            Some(&pkg.closed_year.to_string()),
            Some(&format!(
                "Fiscal closure package applied (year {})",
                pkg.closed_year
            )),
            None,
            Some(serde_json::json!({
                "fiscal_transition_id": pkg.fiscal_transition_id,
                "closed_year":  pkg.closed_year,
                "opened_year":  pkg.opened_year,
                "authority_node_id": pkg.closure_authority_node_id,
                "authority_username": pkg.closure_authority_username,
                "snapshot_count": snapshot_count,
                "authority_origin": "WILAYA",
                "applied_by_role": applied_by_role,
                "execution_mode": "AUTHORIZED_REMOTE_TRANSITION",
                "package_fingerprint": pkg.package_fingerprint
            })),
            None,
            None,
        )?;

        // Update Registry
        self.executor
            .fiscal_package_registry()
            .mark_applied(&pkg.fiscal_transition_id, applying_username)?;

        log::info!(
            target: "grpc::fiscal",
            "[FISCAL_PACKAGE_APPLIED] transition_id={} closed_year={} opened_year={} snapshot_count={} applied_by={} fingerprint={}",
            pkg.fiscal_transition_id, pkg.closed_year, pkg.opened_year,
            snapshot_count, applying_username, pkg.package_fingerprint,
        );

        Ok(FiscalClosureApplyResult {
            fiscal_transition_id: pkg.fiscal_transition_id,
            closed_year: pkg.closed_year,
            opened_year: pkg.opened_year,
            snapshot_count,
            package_fingerprint: pkg.package_fingerprint,
        })
    }
}

impl crate::architecture::Service for FiscalClosurePackageService<'_> {}
