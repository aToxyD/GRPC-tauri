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

use crate::domain::identity::{
    CredentialStatus, IdentityStorePort, SubjectType, SIGNATURE_VERSION_ED25519,
};
use crate::errors::{AppError, BusinessLogicError};
use crate::infrastructure::sync::packages::signing::{
    Ed25519PackageSigner, Ed25519PackageVerifier, PackageSigner, PackageVerifier,
};
use crate::repositories::{DbExecutor, RepositoryProvider};
use serde::{Deserialize, Serialize};

use super::NodeIdentityResolver;

// ─── Package schema version ───────────────────────────────────────────────────
/// Current schema version for `.fiscal-close.sync` packages (SEC-008).
pub const FISCAL_CLOSURE_PACKAGE_VERSION: u32 = 3;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthorizedExecutionWindow {
    pub not_before: String,
    pub expires_at: String,
}

/// Structured content of a fiscal closure authorization package.
/// Serialized as JSON; the Ed25519 signature covers the canonical JSON bytes.
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
    /// identity_id (UUID) of the WILAYA node identity that signed this
    /// package with Ed25519 — covered by the signature and the fingerprint.
    pub issuer_identity_id: String,
    /// Username of the operator who performed `close_year` on Wilaya.
    pub closure_authority_username: String,
    /// UUID — unique per fiscal transition; used for replay protection.
    pub fiscal_transition_id: String,
    /// RFC3339 UTC timestamp when the package file was created.
    pub package_created_at: String,
    /// Hex-encoded Ed25519 public key of the signing WILAYA identity
    /// (SEC-008 — replaces the retired shared HMAC key id).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signing_key_id: Option<String>,
    /// Window in which this package can be applied.
    pub authorized_execution_window: AuthorizedExecutionWindow,
    /// Deterministic SHA256 fingerprint of the canonical package payload.
    pub package_fingerprint: String,
}

/// On-disk envelope: the package JSON + its Ed25519 signature (SEC-008).
#[derive(Debug, Serialize, Deserialize)]
struct FiscalClosureEnvelope {
    pub package: FiscalClosurePackage,
    /// Signature scheme version — must equal `SIGNATURE_VERSION_ED25519` (2).
    pub signature_version: u32,
    /// Hex-encoded Ed25519 public key of the signer (32 bytes).
    pub signer_public_key_hex: String,
    /// Ed25519 signature (hex, 64 bytes) over the canonical JSON of `package`.
    pub signature_hex: String,
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

/// Signer metadata stamped into a fiscal closure package (SEC-008).
///
/// `issuer_identity_id` is the identity_id of the ACTIVE WILAYA identity whose
/// Ed25519 key will sign the package; `signing_key_id` is that key's
/// hex-encoded public key. Both are covered by the signature.
#[derive(Debug, Clone)]
pub struct FiscalClosurePackageSignerInfo {
    pub issuer_identity_id: String,
    pub signing_key_id: String,
}

pub struct FiscalClosurePackageService<'a> {
    executor: DbExecutor<'a>,
}

impl<'a> FiscalClosurePackageService<'a> {
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }

    /// Resolve the local WILAYA signing identity (SEC-008 / ADR-0048).
    ///
    /// Fail-closed: a WILAYA node without a provisioned node key + ACTIVE
    /// WILAYA certificate (or with an R5-mismatched / revoked / non-Ed25519
    /// identity) cannot export fiscal closure packages. No environment-based
    /// signing secret is ever consulted.
    pub fn resolve_wilaya_signer(
        db: &crate::db::Database,
        node_key_store: &crate::infrastructure::identity::NodeKeyStore,
    ) -> Result<(FiscalClosurePackageSignerInfo, Ed25519PackageSigner), AppError> {
        let resolved = NodeIdentityResolver::resolve_local_signer(
            db,
            node_key_store,
            SubjectType::Wilaya,
        )?
        .ok_or_else(|| {
            AppError::BusinessLogic(BusinessLogicError::OperationNotPermitted {
                message:
                    "لا يمكن تصدير حزمة الإغلاق المالي — هوية الولاية غير مُزوّدة على هذه العقدة"
                        .to_string(),
            })
        })?;
        let signer = Ed25519PackageSigner::from_provider(resolved.signer);
        let signer_info = FiscalClosurePackageSignerInfo {
            issuer_identity_id: resolved.certificate.identity_id.to_string(),
            signing_key_id: signer.public_key_hex(),
        };
        Ok((signer_info, signer))
    }

    // ── Signature helpers ────────────────────────────────────────────────────

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
        signer_info: &FiscalClosurePackageSignerInfo,
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
            issuer_identity_id: signer_info.issuer_identity_id.clone(),
            closure_authority_username: closure_authority_username.to_string(),
            fiscal_transition_id,
            package_created_at,
            signing_key_id: Some(signer_info.signing_key_id.clone()),
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
        signer: &Ed25519PackageSigner,
        file_path: &str,
    ) -> Result<(), AppError> {
        let payload = Self::canonical_json(pkg)?;
        let signature_hex = signer.sign(&payload)?;

        let envelope = FiscalClosureEnvelope {
            package: pkg.clone(),
            signature_version: u32::from(signer.signature_version()),
            signer_public_key_hex: signer.public_key_hex(),
            signature_hex,
        };
        let json = serde_json::to_string_pretty(&envelope).map_err(|e| {
            AppError::Internal(format!("fiscal closure envelope serialization: {}", e))
        })?;

        std::fs::write(file_path, json).map_err(|e| {
            AppError::Internal(format!("cannot write fiscal closure package: {}", e))
        })?;

        // Fiscal package registry entry (SEC-008)
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

    /// Read a `.fiscal-close.sync` envelope and verify it fail-closed:
    ///
    /// 1. schema version must be the current one;
    /// 2. `signature_version` must be Ed25519 (`SIGNATURE_VERSION_ED25519`);
    /// 3. signature must be present and a valid 64-byte hex string;
    /// 4. signer public key must be a valid 32-byte hex string;
    /// 5. `issuer_identity_id` must resolve to a known identity;
    /// 6. the issuer must be a WILAYA identity;
    /// 7. the issuer certificate must be ACTIVE;
    /// 8. the issuer certificate must not be expired;
    /// 9. the envelope public key must match the issuer certificate's key;
    /// 10. the issuer must be the trust anchor (ACTIVE WILAYA) of this node;
    /// 11. the Ed25519 signature must verify over the canonical JSON bytes.
    ///
    /// Any failure returns `OperationNotPermitted` — there is no fallback to
    /// any shared-secret scheme (SEC-008 / ADR-0048).
    fn read_and_verify_envelope(&self, file_path: &str) -> Result<FiscalClosurePackage, AppError> {
        let raw = std::fs::read_to_string(file_path).map_err(|e| {
            AppError::Internal(format!("cannot read fiscal closure package file: {}", e))
        })?;

        let envelope: FiscalClosureEnvelope = serde_json::from_str(&raw).map_err(|e| {
            AppError::FileFormat(format!("fiscal closure package is not valid JSON: {}", e))
        })?;

        let reject = |reason: &str, message: &str| -> AppError {
            log::warn!(
                target: "grpc::fiscal",
                "[FISCAL_CLOSURE_PACKAGE_REJECTED] reason={} transition_id={} closed_year={}",
                reason, envelope.package.fiscal_transition_id, envelope.package.closed_year,
            );
            AppError::BusinessLogic(BusinessLogicError::OperationNotPermitted {
                message: message.to_string(),
            })
        };

        // 1: Schema version guard
        if envelope.package.schema_version != FISCAL_CLOSURE_PACKAGE_VERSION {
            return Err(reject(
                "unsupported_schema_version",
                &format!(
                    "نسخة مخطط حزمة الإغلاق المالي غير مدعومة {} (المتوقع {})",
                    envelope.package.schema_version, FISCAL_CLOSURE_PACKAGE_VERSION
                ),
            ));
        }

        // 2: Signature scheme must be Ed25519 — legacy V1/HMAC envelopes and
        // any other scheme are rejected, never silently downgraded.
        if envelope.signature_version != u32::from(SIGNATURE_VERSION_ED25519) {
            return Err(reject(
                "unsupported_signature_version",
                "نسخة توقيع حزمة الإغلاق المالي غير مدعومة (يُتوقّع Ed25519)",
            ));
        }

        // 3: Signature must be present and well-formed
        if envelope.signature_hex.trim().is_empty() {
            return Err(reject(
                "missing_signature",
                "توقيع حزمة الإغلاق المالي مفقود — رُفضت الحزمة",
            ));
        }

        // 4: Signer public key must be well-formed
        let verifier = match Ed25519PackageVerifier::from_hex(&envelope.signer_public_key_hex) {
            Ok(v) => v,
            Err(_) => {
                return Err(reject(
                    "malformed_signer_public_key",
                    "مفتاح توقيع حزمة الإغلاق المالي غير صالح — رُفضت الحزمة",
                ));
            }
        };

        // 5: Issuer identity must be known on this node
        let issuer_id = match envelope.package.issuer_identity_id.parse::<uuid::Uuid>() {
            Ok(id) => id,
            Err(_) => {
                return Err(reject(
                    "malformed_issuer_identity_id",
                    "هوية مُصدِر حزمة الإغلاق المالي غير صالحة — رُفضت الحزمة",
                ));
            }
        };
        let Some(certificate) = self
            .executor
            .identity_store()
            .get_by_identity_id(&issuer_id)?
        else {
            return Err(reject(
                "unknown_signer",
                "هوية مُصدِر حزمة الإغلاق المالي غير معروفة على هذه العقدة — رُفضت الحزمة",
            ));
        };

        // 6: Only a WILAYA identity may authorize a fiscal closure
        if certificate.subject_type != SubjectType::Wilaya {
            return Err(reject(
                "signer_not_wilaya",
                "مُصدِر حزمة الإغلاق المالي ليس هوية ولاية — رُفضت الحزمة",
            ));
        }

        // 7: Issuer certificate must be ACTIVE
        if certificate.status != CredentialStatus::Active {
            return Err(reject(
                "signer_not_active",
                "هوية مُصدِر حزمة الإغلاق المالي ليست نشطة — رُفضت الحزمة",
            ));
        }

        // 8: Issuer certificate must not be expired (advisory expiry; None = no expiry)
        if let Some(not_after) = certificate.not_after {
            if chrono::Utc::now() > not_after {
                return Err(reject(
                    "signer_certificate_expired",
                    "انتهت صلاحية هوية مُصدِر حزمة الإغلاق المالي — رُفضت الحزمة",
                ));
            }
        }

        // 9: Envelope key must match the issuer certificate key (case-insensitive)
        if !certificate
            .public_key
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect::<String>()
            .eq_ignore_ascii_case(&envelope.signer_public_key_hex)
        {
            return Err(reject(
                "signer_key_mismatch",
                "مفتاح توقيع حزمة الإغلاق المالي لا يطابق هوية المُصدِر — رُفضت الحزمة",
            ));
        }

        // 10: Defense-in-depth — the issuer must be the ACTIVE WILAYA trust
        // anchor provisioned on this UNIT (Invariant 6: single ACTIVE WILAYA).
        match self
            .executor
            .identity_store()
            .get_active_by_subject_type(SubjectType::Wilaya)?
        {
            Some(active_wilaya) if active_wilaya.identity_id == issuer_id => {}
            _ => {
                return Err(reject(
                    "signer_not_trusted_anchor",
                    "مُصدِر حزمة الإغلاق المالي غير موثوق على هذه العقدة — رُفضت الحزمة",
                ));
            }
        }

        // 11: Ed25519 signature verification over the canonical JSON bytes
        let payload = Self::canonical_json(&envelope.package)?;
        let signature_ok = verifier
            .verify(&payload, &envelope.signature_hex)
            .map_err(|e| {
                AppError::Internal(format!("fiscal closure signature verification error: {}", e))
            })?;
        if !signature_ok {
            return Err(reject(
                "invalid_signature",
                "فشل التحقق من توقيع حزمة الإغلاق المالي — قد تكون الحزمة قد تعرضت للتلاعب",
            ));
        }

        Ok(envelope.package)
    }

    // ── Unit: pre-flight checks (read-only) ───────────────────────────────────

    pub fn pre_flight_checks(&self, pkg: &FiscalClosurePackage) -> Result<Vec<String>, AppError> {
        let mut issues: Vec<String> = Vec::new();

        // 1: Validate execution window
        let now = chrono::Utc::now();
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

    /// Read, verify Ed25519 signature, run pre-flight checks — do NOT apply anything.
    pub fn preview_closure_package(
        &self,
        file_path: &str,
    ) -> Result<FiscalClosurePreview, AppError> {
        let pkg = self.read_and_verify_envelope(file_path)?;
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
        let pkg = self.read_and_verify_envelope(file_path)?;
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
