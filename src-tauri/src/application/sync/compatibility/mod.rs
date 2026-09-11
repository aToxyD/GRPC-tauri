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
        // SEC-087 Phase 6A (ADR-0057): the import schema window is closed at
        // V3 — V3 is the ONLY supported envelope version. Anything older than
        // V3 (V0/V1/V2) is rejected as too old; anything newer (V4+) is
        // rejected as too new. There is NO backward compatibility window.
        const MIN_SUPPORTED: SchemaVersion = SchemaVersion::V3;
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
    fn accepts_current_schema_version_only() {
        assert!(SupportedSchemaWindow::can_import(SchemaVersion::V3).is_ok());
        assert!(SupportedSchemaWindow::can_import(SYNC_PACKAGE_SCHEMA_VERSION).is_ok());
    }

    #[test]
    fn rejects_v2_as_too_old() {
        let err = SupportedSchemaWindow::can_import(SchemaVersion::V2).expect_err("V2 must reject");
        assert_eq!(err.code(), "PACKAGE_TOO_OLD");
    }

    #[test]
    fn rejects_v1_as_too_old() {
        let err = SupportedSchemaWindow::can_import(SchemaVersion::V1).expect_err("V1 must reject");
        assert_eq!(err.code(), "PACKAGE_TOO_OLD");
    }

    #[test]
    fn rejects_future_schema_as_too_new() {
        let err =
            SupportedSchemaWindow::can_import(SchemaVersion::new(4)).expect_err("V4 must reject");
        assert_eq!(err.code(), "PACKAGE_TOO_NEW");
    }

    #[test]
    fn rejects_older_schema_as_too_old() {
        let err =
            SupportedSchemaWindow::can_import(SchemaVersion::new(0)).expect_err("must reject");
        assert_eq!(err.code(), "PACKAGE_TOO_OLD");
    }
}
