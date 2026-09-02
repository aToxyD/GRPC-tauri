//! Fiscal-year TVA policy service module.
//!
//! Exactly ONE immutable TVA rate per fiscal year (ADR-0055 / SEC-087-F).
//! WILAYA-only set; once established the rate cannot change; `frozen` at
//! fiscal close. Closed/archived fiscal years are immutable.
//! SQL is delegated exclusively to `FiscalYearTaxPolicyRepository`.

use crate::domain::validation::validate_set_tax_policy_request;
use crate::errors::{AppError, BusinessLogicError};
use crate::models::{FiscalYearTaxPolicy, SetTaxPolicyRequest};
use crate::repositories::{DbExecutor, RepositoryProvider};

pub struct FiscalTaxPolicyService<'a> {
    executor: DbExecutor<'a>,
}

impl<'a> FiscalTaxPolicyService<'a> {
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }

    /// Establish the fiscal-year TVA rate (WILAYA only).
    ///
    /// Rules:
    /// - once established, the rate is immutable;
    /// - closed or archived fiscal years are immutable (no policy edits);
    /// - re-setting the identical rate is an idempotent no-op;
    /// - a different rate is always rejected.
    pub fn set_policy(
        &self,
        req: &SetTaxPolicyRequest,
        set_by: &str,
    ) -> Result<FiscalYearTaxPolicy, AppError> {
        validate_set_tax_policy_request(req)?;

        if let Some(status) = self
            .executor
            .fiscal_year_status()
            .get_by_year(req.fiscal_year)?
        {
            if status.status == "closed" || status.archived {
                return Err(AppError::BusinessLogic(
                    BusinessLogicError::FiscalYearClosed {
                        year: req.fiscal_year,
                    },
                ));
            }
        }

        let existing = self
            .executor
            .fiscal_tax_policy()
            .get_policy(req.fiscal_year)?;
        if let Some(policy) = &existing {
            if policy.frozen {
                return Err(AppError::BusinessLogic(
                    BusinessLogicError::OperationNotPermitted {
                        message: format!(
                            "سياسة الضريبة للسنة {} مجمدة — غير قابلة للتعديل",
                            req.fiscal_year
                        ),
                    },
                ));
            }
            if (policy.tva_rate - req.tva_rate).abs() > f64::EPSILON {
                return Err(AppError::BusinessLogic(
                    BusinessLogicError::OperationNotPermitted {
                        message: format!(
                            "معدل الضريبة للسنة {} مثبت مسبقاً ({}%) ولا يمكن تغييره",
                            req.fiscal_year, policy.tva_rate
                        ),
                    },
                ));
            }
            return Ok(policy.clone());
        }

        self.executor
            .fiscal_tax_policy()
            .insert_policy(req, set_by)?;
        Ok(self
            .executor
            .fiscal_tax_policy()
            .get_policy(req.fiscal_year)?
            .expect("inserted policy must exist"))
    }

    /// Freeze the policy for a closing fiscal year (idempotent). Part of the
    /// fiscal-close integration.
    pub fn freeze_policy(&self, fiscal_year: i32) -> Result<(), AppError> {
        if self
            .executor
            .fiscal_tax_policy()
            .policy_exists(fiscal_year)?
        {
            self.executor
                .fiscal_tax_policy()
                .set_frozen(fiscal_year, true)?;
        }
        Ok(())
    }

    /// Read the authoritative TVA rate for a report's fiscal year.
    ///
    /// Returns the policy — consumers compute `base × (1 + tva_rate / 100)`.
    pub fn get_policy(&self, fiscal_year: i32) -> Result<Option<FiscalYearTaxPolicy>, AppError> {
        self.executor.fiscal_tax_policy().get_policy(fiscal_year)
    }

    pub fn list_policies(&self) -> Result<Vec<FiscalYearTaxPolicy>, AppError> {
        self.executor.fiscal_tax_policy().list_policies()
    }

    /// Convenience: the rate expression for a fiscal year, 0.0 when unset.
    pub fn get_tva_rate(&self, fiscal_year: i32) -> Result<f64, AppError> {
        Ok(self
            .executor
            .fiscal_tax_policy()
            .get_policy(fiscal_year)?
            .map(|p| p.tva_rate)
            .unwrap_or(0.0)) // [arch:allow-unwrap-or] see ADR-0007 — Reason: 0.0 is the safe default rate when no fiscal-year policy is configured — not an error condition; Date: 2026-08-09; Owner: Architecture
    }
}

impl crate::architecture::Service for FiscalTaxPolicyService<'_> {}
