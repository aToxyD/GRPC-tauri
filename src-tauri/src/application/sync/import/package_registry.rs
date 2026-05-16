use crate::application::sync::PackageId;
use crate::errors::AppResult;

pub trait ImportedPackageRegistry {
    fn has_imported(&self, package_id: &PackageId) -> AppResult<bool>;
    fn mark_imported(&self, package_id: &PackageId) -> AppResult<()>;
}
