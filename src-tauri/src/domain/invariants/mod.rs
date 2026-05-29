pub mod audit;
pub mod fifo;
pub mod fiscal;
pub mod stock;

pub use audit::*;
pub use fifo::*;
pub use fiscal::*;
pub use stock::*;

/// Core trait implemented by all domain invariants.
///
/// Each invariant is a pure check that verifies a domain rule.
/// Invariants never mutate state and never access infrastructure.
pub trait Invariant {
    type Context: ?Sized;
    type Violation;

    /// Check the invariant against the given context.
    /// Returns all violations found — never short-circuits.
    fn check(ctx: &Self::Context) -> Vec<Self::Violation>;

    /// Returns true if the invariant is satisfied.
    fn is_satisfied(ctx: &Self::Context) -> bool {
        Self::check(ctx).is_empty()
    }
}

/// Wrapper that preserves invariant identity for multi-invariant reports.
#[derive(Debug, Clone, PartialEq)]
pub enum InvariantViolation {
    StockNonNegative(stock::StockNonNegativeViolation),
    FifoOrderStable(fifo::FifoOrderStableViolation),
    OriginImmutable(fiscal::OriginImmutableViolation),
    SingleOpenFiscalYear(fiscal::SingleOpenFiscalYearViolation),
    AuditChainIntegrity(audit::AuditChainIntegrityViolation),
}

/// Run a set of invariants over combined context and collect all violations.
#[derive(Debug, Clone, Default)]
pub struct InvariantReport {
    pub violations: Vec<InvariantViolation>,
}

impl InvariantReport {
    pub fn new() -> Self {
        Self {
            violations: Vec::new(),
        }
    }

    pub fn is_clean(&self) -> bool {
        self.violations.is_empty()
    }

    pub fn add(&mut self, violation: InvariantViolation) {
        self.violations.push(violation);
    }

    pub fn extend(&mut self, other: Self) {
        self.violations.extend(other.violations);
    }

    pub fn count(&self) -> usize {
        self.violations.len()
    }
}
