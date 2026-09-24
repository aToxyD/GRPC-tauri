use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SchemaVersion(u16);

impl SchemaVersion {
    pub const V1: Self = Self(1);
    /// Canonical sorted-key JSON for integrity/signature (ADR 0009).
    pub const V2: Self = Self(2);
    /// SEC-087 Phase 6A (ADR-0057): the current interchange envelope version.
    pub const V3: Self = Self(3);

    pub const fn new(value: u16) -> Self {
        Self(value)
    }

    pub const fn as_u16(self) -> u16 {
        self.0
    }
}

impl core::fmt::Display for SchemaVersion {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.0)
    }
}
