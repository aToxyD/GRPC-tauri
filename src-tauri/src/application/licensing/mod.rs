//! Licensing application services (ADR-0042).
//!
//! Consumes the licensing artifact contract in `infrastructure/licensing`
//! (structure/crypto) and applies the semantic rules the infrastructure layer
//! deliberately does not own: the type registry, entitlement mapping, subject
//! binding evaluation, and the runtime enforcement gate (artifact-spec §5).
//!
//! Layering: services in this module are pure `&Database` orchestrators. They
//! may construct repositories but never hold `AppState`. The node's public key
//! is resolved by the caller (command layer) from `NodeKeyStore`, keeping
//! binding injectable and unit-testable.

pub mod enforcement;
pub mod license_service;
pub mod subject;
#[cfg(test)]
pub mod test_support;
pub mod trust_anchor_service;

pub use enforcement::{enforce, entitlement_for_action};
pub use license_service::LicenseVerificationService;
pub use subject::{evaluate_subject, SubjectBinding};
pub use trust_anchor_service::TrustAnchorService;

impl crate::architecture::Service for LicenseVerificationService {}
impl crate::architecture::Service for TrustAnchorService {}
