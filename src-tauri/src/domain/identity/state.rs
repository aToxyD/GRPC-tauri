//! Identity lifecycle domain model.
//!
//! RFC 2026-08-04-node-identity-trust §3.2 / ADR-0038 §1-2.
//!
//! Explicit credential lifecycle states. `EXPIRED` is issued by the issuing
//! authority and is NEVER derived from a wall clock (Invariant 8).

use serde::{Deserialize, Serialize};

/// Kind of an identity subject.
///
/// ADMIN, UNIT, and WILAYA are identities of the same class; the subject type
/// only describes the referenced entity (`subject_id`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum SubjectType {
    Wilaya,
    Unit,
    Admin,
}

impl SubjectType {
    pub const fn as_str(&self) -> &'static str {
        match self {
            SubjectType::Wilaya => "WILAYA",
            SubjectType::Unit => "UNIT",
            SubjectType::Admin => "ADMIN",
        }
    }
}

impl std::fmt::Display for SubjectType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for SubjectType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "WILAYA" => Ok(SubjectType::Wilaya),
            "UNIT" => Ok(SubjectType::Unit),
            "ADMIN" => Ok(SubjectType::Admin),
            other => Err(format!("Unknown identity subject type: {other}")),
        }
    }
}

/// Explicit credential lifecycle state (ADR-0038 §2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum CredentialStatus {
    Active,
    Revoked,
    Superseded,
    Expired,
}

impl CredentialStatus {
    pub const fn as_str(&self) -> &'static str {
        match self {
            CredentialStatus::Active => "ACTIVE",
            CredentialStatus::Revoked => "REVOKED",
            CredentialStatus::Superseded => "SUPERSEDED",
            CredentialStatus::Expired => "EXPIRED",
        }
    }
}

impl std::fmt::Display for CredentialStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for CredentialStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "ACTIVE" => Ok(CredentialStatus::Active),
            "REVOKED" => Ok(CredentialStatus::Revoked),
            "SUPERSEDED" => Ok(CredentialStatus::Superseded),
            "EXPIRED" => Ok(CredentialStatus::Expired),
            other => Err(format!("Unknown credential status: {other}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subject_type_roundtrip() {
        for ty in [SubjectType::Wilaya, SubjectType::Unit, SubjectType::Admin] {
            let parsed: SubjectType = ty.as_str().parse().unwrap();
            assert_eq!(parsed, ty);
        }
        assert!("WILAYA_ROGUE".parse::<SubjectType>().is_err());
    }

    #[test]
    fn credential_status_roundtrip() {
        for st in [
            CredentialStatus::Active,
            CredentialStatus::Revoked,
            CredentialStatus::Superseded,
            CredentialStatus::Expired,
        ] {
            let parsed: CredentialStatus = st.as_str().parse().unwrap();
            assert_eq!(parsed, st);
        }
        assert!("INVALID".parse::<CredentialStatus>().is_err());
    }
}
