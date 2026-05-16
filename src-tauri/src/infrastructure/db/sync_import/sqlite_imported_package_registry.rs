use crate::application::sync::{ImportedPackageRegistry, PackageId};
use crate::errors::{AppError, AppResult, BusinessLogicError};
use crate::repositories::{DbExecutor, SyncAppliedPackagesRepository};

pub struct SqliteImportedPackageRegistry<'a> {
    repo: SyncAppliedPackagesRepository<'a>,
    kind: &'a str,
    source_node_id: Option<&'a str>,
    imported_by: &'a str,
}

impl<'a> SqliteImportedPackageRegistry<'a> {
    pub fn new(
        executor: DbExecutor<'a>,
        kind: &'a str,
        source_node_id: Option<&'a str>,
        imported_by: &'a str,
    ) -> Self {
        Self {
            repo: SyncAppliedPackagesRepository::new(executor),
            kind,
            source_node_id,
            imported_by,
        }
    }
}

impl ImportedPackageRegistry for SqliteImportedPackageRegistry<'_> {
    fn has_imported(&self, package_id: &PackageId) -> AppResult<bool> {
        self.repo.has_imported(package_id.0.as_str())
    }

    fn mark_imported(&self, package_id: &PackageId) -> AppResult<()> {
        let inserted = self.repo.insert_if_new(
            package_id.0.as_str(),
            self.kind,
            self.source_node_id,
            self.imported_by,
        )?;
        if !inserted {
            return Err(AppError::BusinessLogic(
                BusinessLogicError::DuplicateSyncPackage {
                    package_id: package_id.0.clone(),
                },
            ));
        }
        Ok(())
    }
}
