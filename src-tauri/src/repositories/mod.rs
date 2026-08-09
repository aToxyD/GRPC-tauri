//! Repository layer - Database access abstractions
//!
//! This module provides repository implementations for each domain entity.
//! All repositories use the DbExecutor abstraction to work with both
//! Connection and Transaction contexts.

pub mod anomaly;
pub mod audit;
pub mod domain_events;
pub mod executor;
pub mod fifo_layers;
pub mod fiscal_package_registry;
pub mod fiscal_snapshots;
pub mod fiscal_transitions;
pub mod fiscal_year_status;
pub mod identity_store;
pub mod import_audit_events;
pub mod integrity;
pub mod inventory;
pub mod licensing_anchor;
pub mod licensing_events;
pub mod licensing_license;
pub mod opening_balances;
pub mod orders;
pub mod products;
pub mod rate_limiter;
pub mod registry_snapshots;
pub mod reports;
pub mod sessions;
pub mod settings;
pub mod stock_movements;
pub mod sync_applied_packages;
pub mod sync_conflicts;
pub mod sync_issuer_sequence_state;
pub mod system;
pub mod telemetry;
pub mod timeline;
pub mod units;
pub mod users;

pub use anomaly::AnomalyRepository;
pub use audit::AuditRepository;
pub use domain_events::{DomainEventRepository, EventSequenceGap};
pub use executor::{DbExecutor, ExecutorProvider};
pub use fifo_layers::FifoLayerRepository;
pub use fiscal_package_registry::{FiscalPackageRegistryEntry, FiscalPackageRegistryRepository};
pub use fiscal_snapshots::FiscalSnapshotRepository;
pub use fiscal_transitions::FiscalTransitionRepository;
pub use fiscal_year_status::FiscalYearStatusRepository;
pub use identity_store::IdentityStoreRepository;
pub use import_audit_events::ImportAuditEventsRepository;
pub use integrity::IntegrityRepository;
pub use inventory::InventoryRepository;
pub use licensing_anchor::{LicensingAnchorRepository, LicensingAnchorRow};
pub use licensing_events::{LicensingEventRow, LicensingEventsRepository};
pub use licensing_license::{LicensingLicenseRepository, LicensingLicenseRow};
pub use opening_balances::OpeningBalanceRepository;
pub use orders::OrderRepository;
pub use products::ProductRepository;
pub use rate_limiter::RateLimiterRepository;
pub use registry_snapshots::{RegistrySnapshotRow, RegistrySnapshotsRepository};
pub use reports::ReportRepository;
pub use sessions::SessionRepository;
pub use settings::SettingsRepository;
pub use stock_movements::StockMovementRepository;
pub use sync_applied_packages::SyncAppliedPackagesRepository;
pub use sync_conflicts::SyncConflictRepository;
pub use sync_issuer_sequence_state::{PendingIssuedSequence, SyncIssuerSequenceStateRepository};
pub use system::SystemRepository;
pub use telemetry::TelemetryRepository;
pub use timeline::TimelineRepository;
pub use units::UnitRepository;
pub use users::UserRepository;

impl crate::architecture::Repository for AnomalyRepository<'_> {}
impl crate::architecture::Repository for DomainEventRepository<'_> {}
impl crate::architecture::Repository for UserRepository<'_> {}
impl crate::architecture::Repository for SettingsRepository<'_> {}
impl crate::architecture::Repository for ProductRepository<'_> {}
impl crate::architecture::Repository for StockMovementRepository<'_> {}
impl crate::architecture::Repository for InventoryRepository<'_> {}
impl crate::architecture::Repository for FiscalYearStatusRepository<'_> {}
impl crate::architecture::Repository for FiscalSnapshotRepository<'_> {}
impl crate::architecture::Repository for FiscalTransitionRepository<'_> {}
impl crate::architecture::Repository for ImportAuditEventsRepository<'_> {}
impl crate::architecture::Repository for IdentityStoreRepository<'_> {}
impl crate::architecture::Repository for UnitRepository<'_> {}
impl crate::architecture::Repository for OrderRepository<'_> {}
impl crate::architecture::Repository for ReportRepository<'_> {}
impl crate::architecture::Repository for SyncAppliedPackagesRepository<'_> {}
impl crate::architecture::Repository for SyncIssuerSequenceStateRepository<'_> {}
impl crate::architecture::Repository for AuditRepository<'_> {}
impl crate::architecture::Repository for IntegrityRepository<'_> {}
impl crate::architecture::Repository for SessionRepository<'_> {}
impl crate::architecture::Repository for TelemetryRepository<'_> {}
impl crate::architecture::Repository for TimelineRepository<'_> {}
impl crate::architecture::Repository for FiscalPackageRegistryRepository<'_> {}
impl crate::architecture::Repository for RateLimiterRepository {}
impl crate::architecture::Repository for RegistrySnapshotsRepository<'_> {}
impl crate::architecture::Repository for FifoLayerRepository<'_> {}
impl crate::architecture::Repository for LicensingAnchorRepository<'_> {}
impl crate::architecture::Repository for LicensingLicenseRepository<'_> {}
impl crate::architecture::Repository for LicensingEventsRepository<'_> {}

/// Centralized provider for repositories to avoid manual construction in the service layer.
/// This satisfies Rule 17 of the architectural integrity check.
pub trait RepositoryProvider<'a> {
    fn anomaly(&self) -> AnomalyRepository<'a>;
    fn audit(&self) -> AuditRepository<'a>;
    fn domain_events(&self) -> DomainEventRepository<'a>;
    fn fiscal_year_status(&self) -> FiscalYearStatusRepository<'a>;
    fn fiscal_snapshots(&self) -> FiscalSnapshotRepository<'a>;
    fn fiscal_transitions(&self) -> FiscalTransitionRepository<'a>;
    fn users(&self) -> UserRepository<'a>;
    fn products(&self) -> ProductRepository<'a>;
    fn orders(&self) -> OrderRepository<'a>;
    fn inventory(&self) -> InventoryRepository<'a>;
    fn reports(&self) -> ReportRepository<'a>;
    fn opening_balances(&self) -> OpeningBalanceRepository<'a>;
    fn settings(&self) -> SettingsRepository<'a>;
    fn stock_movements(&self) -> StockMovementRepository<'a>;
    fn units(&self) -> UnitRepository<'a>;
    fn sync_applied_packages(&self) -> SyncAppliedPackagesRepository<'a>;
    fn sync_issuer_sequence_state(&self) -> SyncIssuerSequenceStateRepository<'a>;
    fn import_audit_events(&self) -> ImportAuditEventsRepository<'a>;
    fn identity_store(&self) -> IdentityStoreRepository<'a>;
    fn system(&self) -> SystemRepository<'a>;
    fn sync_conflicts(&self) -> SyncConflictRepository<'a>;
    fn integrity(&self) -> IntegrityRepository<'a>;
    fn sessions(&self) -> SessionRepository<'a>;
    fn telemetry(&self) -> TelemetryRepository<'a>;
    fn timeline(&self) -> TimelineRepository<'a>;
    fn fiscal_package_registry(&self) -> FiscalPackageRegistryRepository<'a>;
    fn fifo_layers(&self) -> FifoLayerRepository<'a>;
    fn registry_snapshots(&self) -> RegistrySnapshotsRepository<'a>;
    fn licensing_anchor(&self) -> LicensingAnchorRepository<'a>;
    fn licensing_license(&self) -> LicensingLicenseRepository<'a>;
    fn licensing_events(&self) -> LicensingEventsRepository<'a>;
}

impl<'a> RepositoryProvider<'a> for DbExecutor<'a> {
    fn anomaly(&self) -> AnomalyRepository<'a> {
        AnomalyRepository::new(*self)
    }
    fn audit(&self) -> AuditRepository<'a> {
        AuditRepository::new(*self)
    }
    fn domain_events(&self) -> DomainEventRepository<'a> {
        DomainEventRepository::new(*self)
    }
    fn fiscal_year_status(&self) -> FiscalYearStatusRepository<'a> {
        FiscalYearStatusRepository::new(*self)
    }
    fn fiscal_snapshots(&self) -> FiscalSnapshotRepository<'a> {
        FiscalSnapshotRepository::new(*self)
    }
    fn fiscal_transitions(&self) -> FiscalTransitionRepository<'a> {
        FiscalTransitionRepository::new(*self)
    }
    fn users(&self) -> UserRepository<'a> {
        UserRepository::new(*self)
    }
    fn products(&self) -> ProductRepository<'a> {
        ProductRepository::new(*self)
    }
    fn orders(&self) -> OrderRepository<'a> {
        OrderRepository::new(*self)
    }
    fn inventory(&self) -> InventoryRepository<'a> {
        InventoryRepository::new(*self)
    }
    fn reports(&self) -> ReportRepository<'a> {
        ReportRepository::new(*self)
    }
    fn opening_balances(&self) -> OpeningBalanceRepository<'a> {
        OpeningBalanceRepository::new(*self)
    }
    fn settings(&self) -> SettingsRepository<'a> {
        SettingsRepository::new(*self)
    }
    fn stock_movements(&self) -> StockMovementRepository<'a> {
        StockMovementRepository::new(*self)
    }
    fn units(&self) -> UnitRepository<'a> {
        UnitRepository::new(*self)
    }
    fn sync_applied_packages(&self) -> SyncAppliedPackagesRepository<'a> {
        SyncAppliedPackagesRepository::new(*self)
    }
    fn sync_issuer_sequence_state(&self) -> SyncIssuerSequenceStateRepository<'a> {
        SyncIssuerSequenceStateRepository::new(*self)
    }
    fn import_audit_events(&self) -> ImportAuditEventsRepository<'a> {
        ImportAuditEventsRepository::new(*self)
    }
    fn identity_store(&self) -> IdentityStoreRepository<'a> {
        IdentityStoreRepository::new(*self)
    }
    fn system(&self) -> SystemRepository<'a> {
        SystemRepository::new(*self)
    }
    fn sync_conflicts(&self) -> SyncConflictRepository<'a> {
        SyncConflictRepository::new(*self)
    }
    fn integrity(&self) -> IntegrityRepository<'a> {
        IntegrityRepository::new(*self)
    }
    fn sessions(&self) -> SessionRepository<'a> {
        SessionRepository::new(*self)
    }
    fn telemetry(&self) -> TelemetryRepository<'a> {
        TelemetryRepository::new(*self)
    }
    fn timeline(&self) -> TimelineRepository<'a> {
        TimelineRepository::new(*self)
    }
    fn fiscal_package_registry(&self) -> FiscalPackageRegistryRepository<'a> {
        FiscalPackageRegistryRepository::new(*self)
    }
    fn fifo_layers(&self) -> FifoLayerRepository<'a> {
        FifoLayerRepository::new(*self)
    }
    fn registry_snapshots(&self) -> RegistrySnapshotsRepository<'a> {
        RegistrySnapshotsRepository::new(*self)
    }
    fn licensing_anchor(&self) -> LicensingAnchorRepository<'a> {
        LicensingAnchorRepository::new(*self)
    }
    fn licensing_license(&self) -> LicensingLicenseRepository<'a> {
        LicensingLicenseRepository::new(*self)
    }
    fn licensing_events(&self) -> LicensingEventsRepository<'a> {
        LicensingEventsRepository::new(*self)
    }
}
