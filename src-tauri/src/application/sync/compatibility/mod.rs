use crate::application::sync::{SchemaVersion, SYNC_PACKAGE_SCHEMA_VERSION};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportCompatibilityError {
    PackageTooOld { version: SchemaVersion },
    PackageTooNew { version: SchemaVersion },
}

impl ImportCompatibilityError {
    pub const fn code(self) -> &'static str {
        match self {
            Self::PackageTooOld { .. } => "PACKAGE_TOO_OLD",
            Self::PackageTooNew { .. } => "PACKAGE_TOO_NEW",
        }
    }
}

pub trait CompatibilityPolicy {
    fn can_import(version: SchemaVersion) -> Result<(), ImportCompatibilityError>;
}

pub struct SupportedSchemaWindow;

impl CompatibilityPolicy for SupportedSchemaWindow {
    fn can_import(version: SchemaVersion) -> Result<(), ImportCompatibilityError> {
        const MIN_SUPPORTED: SchemaVersion = SchemaVersion::V1;
        let max = SYNC_PACKAGE_SCHEMA_VERSION;
        if version < MIN_SUPPORTED {
            return Err(ImportCompatibilityError::PackageTooOld { version });
        }
        if version > max {
            return Err(ImportCompatibilityError::PackageTooNew { version });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_v1_and_v2_when_exporter_emits_v2() {
        assert!(SupportedSchemaWindow::can_import(SchemaVersion::V1).is_ok());
        assert!(SupportedSchemaWindow::can_import(SchemaVersion::V2).is_ok());
        assert!(SupportedSchemaWindow::can_import(SYNC_PACKAGE_SCHEMA_VERSION).is_ok());
    }

    #[test]
    fn rejects_future_schema_as_too_new() {
        let err =
            SupportedSchemaWindow::can_import(SchemaVersion::new(3)).expect_err("must reject");
        assert_eq!(err.code(), "PACKAGE_TOO_NEW");
    }

    #[test]
    fn rejects_older_schema_as_too_old() {
        let err =
            SupportedSchemaWindow::can_import(SchemaVersion::new(0)).expect_err("must reject");
        assert_eq!(err.code(), "PACKAGE_TOO_OLD");
    }
}
