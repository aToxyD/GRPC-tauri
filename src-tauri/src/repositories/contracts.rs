//! Contracts Repository Module
//!
//! Handles contract, contract product, contract allocation and contract
//! allocation exception persistence (ADR-0055 / SEC-087-F).
//! ARCHITECTURE: SQL only — no loops, no calculations, no cross-repo calls.
//! Concurrency: single-writer; correctness is enforced by conditional
//! guarded UPDATEs against the component quantity CHECK constraint.

use crate::errors::AppError;
use crate::models::{
    AddContractProductRequest, Contract, ContractAllocation, ContractAllocationException,
    ContractProduct, CreateContractRequest, ReleaseReasonCode, UnitEntitlementRow,
};
use crate::repositories::executor::DbExecutor;
use rusqlite::{params, Row};

fn map_contract_row(row: &Row<'_>) -> Result<Contract, rusqlite::Error> {
    let created_at_str: Option<String> = row.get(11)?;
    let created_at = match created_at_str {
        Some(s) => crate::errors::parse_datetime_rfc3339(&s)
            .map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    11,
                    rusqlite::types::Type::Text,
                    Box::new(e),
                )
            })?
            .into(),
        None => None,
    };
    Ok(Contract {
        id: row.get(0)?,
        contract_reference: row.get(1)?,
        unit_id: row.get(2)?,
        supplier_id: row.get(3)?,
        fiscal_year: row.get(4)?,
        status: crate::models::ContractStatus::from(row.get::<_, String>(5)?),
        proposed_at: row.get(6)?,
        accepted_at: row.get(7)?,
        activated_at: row.get(8)?,
        ended_at: row.get(9)?,
        cancelled_at: row.get(10)?,
        notes: row.get(12)?,
        created_at,
    })
}

const CONTRACT_COLUMNS: &str = "id, contract_reference, unit_id, supplier_id, fiscal_year, status, proposed_at, accepted_at, activated_at, ended_at, cancelled_at, created_at, notes";

fn map_contract_product_row(row: &Row<'_>) -> Result<ContractProduct, rusqlite::Error> {
    Ok(ContractProduct {
        id: row.get(0)?,
        contract_id: row.get(1)?,
        product_id: row.get(2)?,
        product_name: row.get(3)?,
        proposed_price: row.get(4)?,
        agreed_price: row.get(5)?,
    })
}

const CONTRACT_PRODUCT_COLUMNS: &str =
    "cp.id, cp.contract_id, cp.product_id, p.name, cp.proposed_price, cp.agreed_price";

fn map_allocation_row(row: &Row<'_>) -> Result<ContractAllocation, rusqlite::Error> {
    Ok(ContractAllocation {
        id: row.get(0)?,
        contract_id: row.get(1)?,
        contract_product_id: row.get(2)?,
        unit_id: row.get(3)?,
        product_id: row.get(4)?,
        fiscal_year: row.get(5)?,
        contracted_quantity: row.get(6)?,
        fulfilled_quantity: row.get(7)?,
        released_quantity: row.get(8)?,
        reserved_quantity: row.get(9)?,
        entitlement_state: row.get(10)?,
        version: row.get(11)?,
    })
}

const ALLOCATION_COLUMNS: &str = "id, contract_id, contract_product_id, unit_id, product_id, fiscal_year, contracted_quantity, fulfilled_quantity, released_quantity, reserved_quantity, entitlement_state, version";

fn map_exception_row(row: &Row<'_>) -> Result<ContractAllocationException, rusqlite::Error> {
    Ok(ContractAllocationException {
        id: row.get(0)?,
        allocation_id: row.get(1)?,
        released_quantity: row.get(2)?,
        reason_code: row.get(3)?,
        reason_note: row.get(4)?,
        created_by: row.get(5)?,
        created_at: row.get(6)?,
    })
}

/// Repository for contract-related database operations
pub struct ContractRepository<'a> {
    executor: DbExecutor<'a>,
}

impl<'a> ContractRepository<'a> {
    /// Create a new ContractRepository with the given executor
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }

    // ------------------------------------------------------------------
    // Contracts
    // ------------------------------------------------------------------

    pub fn insert_contract(
        &self,
        id: &str,
        req: &CreateContractRequest,
        created_at: &str,
    ) -> Result<(), AppError> {
        self.executor.execute(
            "INSERT INTO contracts (id, contract_reference, unit_id, supplier_id, fiscal_year, status, proposed_at, created_at) VALUES (?1, ?2, ?3, ?4, ?5, 'proposed', ?6, ?7)",
            params![
                id,
                &req.contract_reference,
                &req.unit_id,
                &req.supplier_id,
                &req.fiscal_year,
                created_at,
                created_at,
            ],
        )?;
        Ok(())
    }

    pub fn get_contract(&self, contract_id: &str) -> Result<Option<Contract>, AppError> {
        let result = self.executor.query_row_optional(
            &format!("SELECT {CONTRACT_COLUMNS} FROM contracts WHERE id = ?1"),
            [contract_id],
            map_contract_row,
        )?;
        Ok(result)
    }

    pub fn list_contracts(
        &self,
        unit_id: Option<&str>,
        supplier_id: Option<&str>,
        fiscal_year: Option<i32>,
    ) -> Result<Vec<Contract>, AppError> {
        let mut sql = format!("SELECT {CONTRACT_COLUMNS} FROM contracts WHERE deleted = 0");
        let mut filters: Vec<String> = Vec::new();
        let mut keyed: Vec<String> = Vec::new();
        if let Some(u) = unit_id {
            filters.push(format!("unit_id = '{}'", u.replace('\'', "''")));
            keyed.push("unit_id".to_string());
        }
        if let Some(s) = supplier_id {
            filters.push(format!("supplier_id = '{}'", s.replace('\'', "''")));
            keyed.push("supplier_id".to_string());
        }
        if let Some(y) = fiscal_year {
            filters.push(format!("fiscal_year = {y}"));
        }
        if !filters.is_empty() {
            sql.push_str(" AND ");
            sql.push_str(&filters.join(" AND "));
        }
        sql.push_str(" ORDER BY fiscal_year DESC, created_at DESC");
        Ok(self.executor.query_all(&sql, [], map_contract_row)?)
    }

    /// Count live (proposed/accepted/active) contracts for a unit + fiscal year.
    pub fn count_live_contracts_for_unit_year(
        &self,
        unit_id: &str,
        fiscal_year: i32,
    ) -> Result<u32, AppError> {
        let count: i64 = self.executor.query_row(
            "SELECT COUNT(*) FROM contracts WHERE unit_id = ?1 AND fiscal_year = ?2 AND deleted = 0 AND status IN ('proposed','accepted','active')",
            params![unit_id, fiscal_year],
            |row| row.get(0),
        )?;
        Ok(count as u32)
    }

    /// Guarded status transition: only matches the expected current status.
    pub fn update_contract_status(
        &self,
        contract_id: &str,
        expected_statuses: &[&str],
        new_status: &str,
        status_column: &str,
        at: &str,
    ) -> Result<usize, AppError> {
        let placeholders = expected_statuses
            .iter()
            .map(|s| format!("'{s}'"))
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "UPDATE contracts SET status = '{}', {} = '{}' WHERE id = ?1 AND deleted = 0 AND status IN ({})",
            new_status, status_column, at, placeholders
        );
        let n = self.executor.execute(&sql, [contract_id])?;
        Ok(n)
    }

    /// Append notes (allowed while proposed).
    pub fn update_contract_notes(
        &self,
        contract_id: &str,
        notes: &Option<String>,
        expected_statuses: &[&str],
    ) -> Result<usize, AppError> {
        let placeholders = expected_statuses
            .iter()
            .map(|s| format!("'{s}'"))
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "UPDATE contracts SET notes = ?1 WHERE id = ?2 AND deleted = 0 AND status IN ({})",
            placeholders
        );
        let n = self.executor.execute(&sql, params![notes, contract_id])?;
        Ok(n)
    }

    // ------------------------------------------------------------------
    // Contract products
    // ------------------------------------------------------------------

    pub fn insert_contract_product(
        &self,
        id: &str,
        req: &AddContractProductRequest,
        created_at: &str,
    ) -> Result<(), AppError> {
        self.executor.execute(
            "INSERT INTO contract_products (id, contract_id, product_id, proposed_price, agreed_price, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                id,
                &req.contract_id,
                &req.product_id,
                &req.proposed_price,
                &req.agreed_price,
                created_at
            ],
        )?;
        Ok(())
    }

    pub fn get_contract_product(
        &self,
        contract_product_id: &str,
    ) -> Result<Option<ContractProduct>, AppError> {
        let result = self
            .executor
            .query_row_optional(
                &format!(
                    "SELECT {CONTRACT_PRODUCT_COLUMNS} FROM contract_products cp JOIN products p ON p.id = cp.product_id WHERE cp.id = ?1"
                ),
                [contract_product_id],
                map_contract_product_row,
            )?;
        Ok(result)
    }

    pub fn get_contract_products(
        &self,
        contract_id: &str,
    ) -> Result<Vec<ContractProduct>, AppError> {
        Ok(self.executor.query_all(
            &format!(
                "SELECT {CONTRACT_PRODUCT_COLUMNS} FROM contract_products cp JOIN products p ON p.id = cp.product_id WHERE cp.contract_id = ?1 AND cp.deleted = 0 ORDER BY p.name"
            ),
            [contract_id],
            map_contract_product_row,
        )?)
    }

    /// Contract product rows plus their authoritative `created_at` (sync
    /// dataset export — the `ContractProduct` model does not expose it).
    pub fn list_sync_contract_products(
        &self,
        contract_id: &str,
    ) -> Result<Vec<(ContractProduct, String)>, AppError> {
        Ok(self.executor.query_all(
            "SELECT cp.id, cp.contract_id, cp.product_id, p.name, cp.proposed_price, cp.agreed_price, cp.created_at
             FROM contract_products cp JOIN products p ON p.id = cp.product_id
             WHERE cp.contract_id = ?1 AND cp.deleted = 0 ORDER BY p.name",
            [contract_id],
            |row| {
                Ok((
                    ContractProduct {
                        id: row.get(0)?,
                        contract_id: row.get(1)?,
                        product_id: row.get(2)?,
                        product_name: row.get(3)?,
                        proposed_price: row.get(4)?,
                        agreed_price: row.get(5)?,
                    },
                    row.get::<_, String>(6)?,
                ))
            },
        )?)
    }

    /// Freeze the agreed price. Only allowed while the contract is `proposed`;
    /// after acceptance the price is immutable.
    pub fn set_agreed_price(
        &self,
        contract_product_id: &str,
        agreed_price: f64,
    ) -> Result<usize, AppError> {
        let n = self.executor.execute(
            "UPDATE contract_products SET agreed_price = ?1 WHERE id = ?2 AND contract_id IN (SELECT id FROM contracts WHERE status = 'proposed' AND deleted = 0)",
            params![agreed_price, contract_product_id],
        )?;
        Ok(n)
    }

    pub fn product_in_contract(
        &self,
        contract_id: &str,
        product_id: &str,
    ) -> Result<bool, AppError> {
        let existing: Option<String> = self.executor.query_row_optional(
            "SELECT id FROM contract_products WHERE contract_id = ?1 AND product_id = ?2",
            params![contract_id, product_id],
            |row| row.get(0),
        )?;
        Ok(existing.is_some())
    }

    // ------------------------------------------------------------------
    // Contract allocations (obligation ledger)
    // ------------------------------------------------------------------

    #[allow(clippy::too_many_arguments)]
    pub fn insert_allocation(
        &self,
        id: &str,
        contract_id: &str,
        contract_product_id: &str,
        unit_id: &str,
        product_id: &str,
        fiscal_year: i32,
        contracted_quantity: f64,
        created_at: &str,
    ) -> Result<(), AppError> {
        self.executor.execute(
            "INSERT INTO contract_allocations (id, contract_id, contract_product_id, unit_id, product_id, fiscal_year, contracted_quantity, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                id,
                contract_id,
                contract_product_id,
                unit_id,
                product_id,
                fiscal_year,
                contracted_quantity,
                created_at
            ],
        )?;
        Ok(())
    }

    pub fn get_allocation(
        &self,
        allocation_id: &str,
    ) -> Result<Option<ContractAllocation>, AppError> {
        let result = self.executor.query_row_optional(
            &format!("SELECT {ALLOCATION_COLUMNS} FROM contract_allocations WHERE id = ?1"),
            [allocation_id],
            map_allocation_row,
        )?;
        Ok(result)
    }

    pub fn list_allocations_for_contract(
        &self,
        contract_id: &str,
    ) -> Result<Vec<ContractAllocation>, AppError> {
        Ok(self.executor.query_all(
            &format!("SELECT {ALLOCATION_COLUMNS} FROM contract_allocations WHERE contract_id = ?1 AND deleted = 0"),
            [contract_id],
            map_allocation_row,
        )?)
    }

    /// Every non-deleted `contract_allocations` row (fleet-wide entitlement
    /// projection for the WILAYA Excel export — ADR-0055 / SEC-087-F).
    pub fn list_all_allocations(&self) -> Result<Vec<ContractAllocation>, AppError> {
        Ok(self.executor.query_all(
            &format!("SELECT {ALLOCATION_COLUMNS} FROM contract_allocations WHERE deleted = 0"),
            [],
            map_allocation_row,
        )?)
    }

    /// Contract allocation rows plus their authoritative `created_at` (sync
    /// dataset export — the `ContractAllocation` model does not expose it).
    pub fn list_sync_allocations_for_contract(
        &self,
        contract_id: &str,
    ) -> Result<Vec<(ContractAllocation, String)>, AppError> {
        Ok(self.executor.query_all(
            "SELECT id, contract_id, contract_product_id, unit_id, product_id, fiscal_year,
                    contracted_quantity, fulfilled_quantity, released_quantity, reserved_quantity,
                    entitlement_state, version, created_at
             FROM contract_allocations WHERE contract_id = ?1 AND deleted = 0",
            [contract_id],
            |row| {
                Ok((
                    ContractAllocation {
                        id: row.get(0)?,
                        contract_id: row.get(1)?,
                        contract_product_id: row.get(2)?,
                        unit_id: row.get(3)?,
                        product_id: row.get(4)?,
                        fiscal_year: row.get(5)?,
                        contracted_quantity: row.get(6)?,
                        fulfilled_quantity: row.get(7)?,
                        released_quantity: row.get(8)?,
                        reserved_quantity: row.get(9)?,
                        entitlement_state: row.get(10)?,
                        version: row.get(11)?,
                    },
                    row.get::<_, String>(12)?,
                ))
            },
        )?)
    }

    pub fn get_allocation_by_contract_product(
        &self,
        contract_product_id: &str,
    ) -> Result<Option<ContractAllocation>, AppError> {
        let result = self
            .executor
            .query_row_optional(
                &format!("SELECT {ALLOCATION_COLUMNS} FROM contract_allocations WHERE contract_product_id = ?1 AND deleted = 0"),
                [contract_product_id],
                map_allocation_row,
            )?;
        Ok(result)
    }

    /// Resolution candidates for (unit, product): all lifecycle-valid rows
    /// ordered oldest obligations first (fiscal_year ASC, created_at ASC, id ASC).
    /// CANCELLED entitlements are excluded (they are not obligations).
    pub fn list_resolution_candidates(
        &self,
        unit_id: &str,
        product_id: &str,
    ) -> Result<Vec<ContractAllocation>, AppError> {
        Ok(self.executor.query_all(
            &format!(
                "SELECT {ALLOCATION_COLUMNS} FROM contract_allocations
                 WHERE unit_id = ?1 AND product_id = ?2 AND deleted = 0
                   AND entitlement_state != 'CANCELLED'
                 ORDER BY fiscal_year ASC, created_at ASC, id ASC"
            ),
            params![unit_id, product_id],
            map_allocation_row,
        )?)
    }

    /// List ACTIVE entitlement allocations for a unit within a fiscal year.
    pub fn list_active_allocations_for_unit_year(
        &self,
        unit_id: &str,
        fiscal_year: i32,
    ) -> Result<Vec<ContractAllocation>, AppError> {
        Ok(self.executor.query_all(
            &format!(
                "SELECT {ALLOCATION_COLUMNS} FROM contract_allocations
                 WHERE unit_id = ?1 AND fiscal_year = ?2 AND deleted = 0 AND entitlement_state = 'ACTIVE'"
            ),
            params![unit_id, fiscal_year],
            map_allocation_row,
        )?)
    }

    /// Full resolution candidates for (unit, product): allocation + owning
    /// contract's supplier + agreed price, ordered oldest obligations first
    /// (fiscal_year ASC, created_at ASC, id ASC). CANCELLED excluded.
    pub fn list_resolution_candidates_full(
        &self,
        unit_id: &str,
        product_id: &str,
    ) -> Result<Vec<crate::domain::pricing::resolver::ResolutionCandidate>, AppError> {
        Ok(self.executor.query_all(
            "SELECT ca.id, ca.fiscal_year, c.supplier_id, s.name, cp.agreed_price,
                    ca.contracted_quantity, ca.fulfilled_quantity, ca.released_quantity,
                    ca.reserved_quantity, ca.entitlement_state
             FROM contract_allocations ca
             JOIN contracts c ON c.id = ca.contract_id
             JOIN suppliers s ON s.id = c.supplier_id
             JOIN contract_products cp ON cp.id = ca.contract_product_id
             WHERE ca.unit_id = ?1 AND ca.product_id = ?2 AND ca.deleted = 0
               AND ca.entitlement_state != 'CANCELLED'
             ORDER BY ca.fiscal_year ASC, ca.created_at ASC, ca.id ASC",
            params![unit_id, product_id],
            |row| {
                Ok(crate::domain::pricing::resolver::ResolutionCandidate {
                    allocation_id: row.get(0)?,
                    allocation_fiscal_year: row.get(1)?,
                    supplier_id: row.get(2)?,
                    supplier_name: row.get(3)?,
                    agreed_price: row.get(4)?,
                    contracted_quantity: row.get(5)?,
                    fulfilled_quantity: row.get(6)?,
                    released_quantity: row.get(7)?,
                    reserved_quantity: row.get(8)?,
                    entitlement_state: row.get(9)?,
                })
            },
        )?)
    }

    /// Read-only UNIT projection of the locally imported ContractCatalog
    /// allocations (Phase 4). Scoped to a single server-derived `unit_id`.
    ///
    /// Central to the design: `ca.fiscal_year ASC, c.created_at ASC, ca.id ASC`
    /// is DISPLAY ordering only — it is NOT resolver/priority ordering and must
    /// never be interpreted as supplier-selection logic. CANCELLED rows are
    /// kept (they are part of the lifecycle projection, unlike the order
    /// resolver which excludes them). Repository performs no arithmetic:
    /// `effective_remaining` is derived by the application service via the
    /// single-source domain helper.
    pub fn list_unit_entitlements(
        &self,
        unit_id: &str,
    ) -> Result<Vec<UnitEntitlementRow>, AppError> {
        Ok(self.executor.query_all(
            "SELECT cp.product_id, p.name, c.supplier_id, s.name,
                    ca.fiscal_year,
                    ca.contracted_quantity, ca.fulfilled_quantity, ca.released_quantity,
                    ca.reserved_quantity, ca.entitlement_state, c.status,
                    cp.agreed_price
             FROM contract_allocations ca
             JOIN contracts c ON c.id = ca.contract_id
             JOIN suppliers s ON s.id = c.supplier_id
             JOIN contract_products cp ON cp.id = ca.contract_product_id
             JOIN products p ON p.id = cp.product_id
             WHERE ca.unit_id = ?1 AND ca.deleted = 0
             ORDER BY ca.fiscal_year ASC, c.created_at ASC, ca.id ASC",
            params![unit_id],
            |row| {
                Ok(UnitEntitlementRow {
                    product_id: row.get(0)?,
                    product_name: row.get(1)?,
                    supplier_id: row.get(2)?,
                    supplier_name: row.get(3)?,
                    fiscal_year: row.get(4)?,
                    contracted_quantity: row.get(5)?,
                    fulfilled_quantity: row.get(6)?,
                    released_quantity: row.get(7)?,
                    reserved_quantity: row.get(8)?,
                    entitlement_state: row.get(9)?,
                    contract_status: row.get(10)?,
                    agreed_price: row.get(11)?,
                })
            },
        )?)
    }

    // ------------------------------------------------------------------
    // Conditional quantity updates (single-writer correctness)
    // ------------------------------------------------------------------

    /// Guarded fulfillment increment for order confirmation. Returns affected
    /// row count (0 => would breach contracted quantity => caller aborts tx).
    pub fn try_increment_fulfilled(
        &self,
        allocation_id: &str,
        quantity: f64,
    ) -> Result<usize, AppError> {
        let n = self.executor.execute(
            "UPDATE contract_allocations
             SET fulfilled_quantity = fulfilled_quantity + ?1,
                 version = version + 1
             WHERE id = ?2 AND deleted = 0
               AND fulfilled_quantity + ?1 + released_quantity + reserved_quantity <= contracted_quantity",
            params![quantity, allocation_id],
        )?;
        Ok(n)
    }

    /// Guarded reservation increment for order creation. Reserved quantity is
    /// converted to fulfillment at confirmation; the guard prevents reservation
    /// beyond the contracted quantity.
    pub fn try_increment_reserved(
        &self,
        allocation_id: &str,
        quantity: f64,
    ) -> Result<usize, AppError> {
        let n = self.executor.execute(
            "UPDATE contract_allocations
             SET reserved_quantity = reserved_quantity + ?1,
                 version = version + 1
             WHERE id = ?2 AND deleted = 0
               AND fulfilled_quantity + ?1 + released_quantity + reserved_quantity <= contracted_quantity",
            params![quantity, allocation_id],
        )?;
        Ok(n)
    }

    /// Convert reservation to fulfillment atomically at confirmation.
    /// Returns affected row count (0 => guard failed => abort tx).
    pub fn try_convert_reserved_to_fulfilled(
        &self,
        allocation_id: &str,
        quantity: f64,
    ) -> Result<usize, AppError> {
        let n = self.executor.execute(
            "UPDATE contract_allocations
             SET fulfilled_quantity = fulfilled_quantity + ?1,
                 reserved_quantity = reserved_quantity - ?1,
                 version = version + 1
             WHERE id = ?2 AND deleted = 0
               AND reserved_quantity >= ?1
               AND fulfilled_quantity + ?1 + released_quantity + reserved_quantity <= contracted_quantity",
            params![quantity, allocation_id],
        )?;
        Ok(n)
    }

    /// Guarded reservation release (order deletion/cancellation).
    pub fn try_release_reserved(
        &self,
        allocation_id: &str,
        quantity: f64,
    ) -> Result<usize, AppError> {
        let n = self.executor.execute(
            "UPDATE contract_allocations
             SET reserved_quantity = reserved_quantity - ?1,
                 version = version + 1
             WHERE id = ?2 AND deleted = 0 AND reserved_quantity >= ?1",
            params![quantity, allocation_id],
        )?;
        Ok(n)
    }

    /// WILAYA-authorized release increment (only via a recorded exception).
    pub fn try_increment_released(
        &self,
        allocation_id: &str,
        quantity: f64,
    ) -> Result<usize, AppError> {
        let n = self.executor.execute(
            "UPDATE contract_allocations
             SET released_quantity = released_quantity + ?1,
                 version = version + 1
             WHERE id = ?2 AND deleted = 0
               AND fulfilled_quantity + released_quantity + reserved_quantity + ?1 <= contracted_quantity",
            params![quantity, allocation_id],
        )?;
        Ok(n)
    }

    /// Revoke a release: only when the released quantity is still fully
    /// unreleased by fulfillment beyond it (i.e. never negative).
    pub fn try_decrement_released(
        &self,
        allocation_id: &str,
        quantity: f64,
    ) -> Result<usize, AppError> {
        let n = self.executor.execute(
            "UPDATE contract_allocations
             SET released_quantity = released_quantity - ?1,
                 version = version + 1
             WHERE id = ?2 AND deleted = 0 AND released_quantity >= ?1",
            params![quantity, allocation_id],
        )?;
        Ok(n)
    }

    pub fn set_entitlement_state(
        &self,
        allocation_id: &str,
        state: &str,
    ) -> Result<usize, AppError> {
        let n = self.executor.execute(
            "UPDATE contract_allocations
             SET entitlement_state = ?1, version = version + 1
             WHERE id = ?2 AND deleted = 0",
            params![state, allocation_id],
        )?;
        Ok(n)
    }

    // ------------------------------------------------------------------
    // Contract allocation exceptions (WILAYA release records)
    // ------------------------------------------------------------------

    #[allow(clippy::too_many_arguments)]
    pub fn insert_exception(
        &self,
        id: &str,
        allocation_id: &str,
        reason_code: ReleaseReasonCode,
        released_quantity: f64,
        reason_note: &Option<String>,
        created_by: &str,
        created_at: &str,
    ) -> Result<(), AppError> {
        self.executor.execute(
            "INSERT INTO contract_allocation_exceptions (id, allocation_id, released_quantity, reason_code, reason_note, created_by, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                id,
                allocation_id,
                released_quantity,
                reason_code.to_string(),
                reason_note,
                created_by,
                created_at
            ],
        )?;
        Ok(())
    }

    pub fn get_exception(
        &self,
        exception_id: &str,
    ) -> Result<Option<ContractAllocationException>, AppError> {
        let result = self
            .executor
            .query_row_optional(
                "SELECT id, allocation_id, released_quantity, reason_code, reason_note, created_by, created_at FROM contract_allocation_exceptions WHERE id = ?1",
                [exception_id],
                map_exception_row,
            )?;
        Ok(result)
    }

    pub fn list_exceptions_for_allocation(
        &self,
        allocation_id: &str,
    ) -> Result<Vec<ContractAllocationException>, AppError> {
        Ok(self.executor.query_all(
            "SELECT id, allocation_id, released_quantity, reason_code, reason_note, created_by, created_at FROM contract_allocation_exceptions WHERE allocation_id = ?1 ORDER BY created_at",
            [allocation_id],
            map_exception_row,
        )?)
    }

    pub fn delete_exception(&self, exception_id: &str) -> Result<usize, AppError> {
        let n = self.executor.execute(
            "DELETE FROM contract_allocation_exceptions WHERE id = ?1",
            [exception_id],
        )?;
        Ok(n)
    }

    // ------------------------------------------------------------------
    // Sync upserts (ContractCatalog V2, WILAYA → UNIT read-only projection)
    //
    // WILAYA is the single source of truth for every WILAYA-owned column. On
    // conflict the WILAYA-owned fields are refreshed unconditionally; the
    // UNIT-local `fulfilled_quantity` / `reserved_quantity` are NEVER touched
    // by a sync package (they are UNIT-owned runtime state). The CHECK
    // constraint (`fulfilled + released + reserved <= contracted`) makes a
    // conflicting reduction fail loud instead of silently corrupting state.
    // ------------------------------------------------------------------

    pub fn upsert_sync_contract(&self, contract: &Contract) -> Result<(), AppError> {
        let created_at = contract
            .created_at
            .ok_or_else(|| {
                AppError::Validation(crate::errors::ValidationError::Required {
                    field: "created_at".into(),
                })
            })?
            .to_rfc3339();
        self.executor.execute(
            "INSERT INTO contracts (id, contract_reference, unit_id, supplier_id, fiscal_year, status,
                                    proposed_at, accepted_at, activated_at, ended_at, cancelled_at,
                                    notes, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
             ON CONFLICT(id) DO UPDATE SET
                contract_reference = excluded.contract_reference,
                unit_id = excluded.unit_id,
                supplier_id = excluded.supplier_id,
                fiscal_year = excluded.fiscal_year,
                status = excluded.status,
                proposed_at = excluded.proposed_at,
                accepted_at = excluded.accepted_at,
                activated_at = excluded.activated_at,
                ended_at = excluded.ended_at,
                cancelled_at = excluded.cancelled_at,
                notes = excluded.notes",
            params![
                contract.id,
                contract.contract_reference,
                contract.unit_id,
                contract.supplier_id,
                contract.fiscal_year,
                contract.status.to_string(),
                contract.proposed_at,
                contract.accepted_at,
                contract.activated_at,
                contract.ended_at,
                contract.cancelled_at,
                contract.notes,
                created_at,
            ],
        )?;
        Ok(())
    }

    pub fn upsert_sync_contract_product(
        &self,
        id: &str,
        contract_id: &str,
        product_id: &str,
        proposed_price: f64,
        agreed_price: Option<f64>,
        created_at: &str,
    ) -> Result<(), AppError> {
        self.executor.execute(
            "INSERT INTO contract_products (id, contract_id, product_id, proposed_price, agreed_price, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(id) DO UPDATE SET
                proposed_price = excluded.proposed_price,
                agreed_price = excluded.agreed_price",
            params![id, contract_id, product_id, proposed_price, agreed_price, created_at],
        )?;
        Ok(())
    }

    pub fn upsert_sync_allocation(
        &self,
        allocation: &ContractAllocation,
        created_at: &str,
    ) -> Result<(), AppError> {
        self.executor.execute(
            "INSERT INTO contract_allocations (id, contract_id, contract_product_id, unit_id, product_id,
                                                fiscal_year, contracted_quantity, fulfilled_quantity,
                                                released_quantity, reserved_quantity, entitlement_state,
                                                version, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
             ON CONFLICT(id) DO UPDATE SET
                contract_id = excluded.contract_id,
                contracted_quantity = excluded.contracted_quantity,
                released_quantity = excluded.released_quantity,
                entitlement_state = excluded.entitlement_state,
                version = excluded.version",
            params![
                allocation.id,
                allocation.contract_id,
                allocation.contract_product_id,
                allocation.unit_id,
                allocation.product_id,
                allocation.fiscal_year,
                allocation.contracted_quantity,
                allocation.fulfilled_quantity,
                allocation.released_quantity,
                allocation.reserved_quantity,
                allocation.entitlement_state,
                allocation.version,
                created_at,
            ],
        )?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn upsert_sync_exception(
        &self,
        id: &str,
        allocation_id: &str,
        released_quantity: f64,
        reason_code: &str,
        reason_note: &Option<String>,
        created_by: &str,
        created_at: &str,
    ) -> Result<(), AppError> {
        self.executor.execute(
            "INSERT INTO contract_allocation_exceptions (id, allocation_id, released_quantity, reason_code, reason_note, created_by, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(id) DO NOTHING",
            params![
                id,
                allocation_id,
                released_quantity,
                reason_code,
                reason_note,
                created_by,
                created_at,
            ],
        )?;
        Ok(())
    }
}
