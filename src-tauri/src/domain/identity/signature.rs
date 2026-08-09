//! Ed25519 certificate signature — fixed-size strong type.
//!
//! RFC 2026-08-04-node-identity-trust §3.3.1 / ADR-0038 / ADR-0039 §6.
//!
//! An Ed25519 signature is exactly 64 bytes. The domain enforces that length at
//! the boundary so that invalid signature material is a compile-time/type error
//! rather than a runtime check scattered through the code. Storage layers may
//! keep raw bytes (`Vec<u8>`/BLOB); conversion to this type validates the length.

use std::fmt;

/// A verified-format Ed25519 certificate signature (RFC 8032).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Ed25519CertificateSignature([u8; 64]);

impl Ed25519CertificateSignature {
    /// Canonical byte length of an Ed25519 signature.
    pub const LEN: usize = 64;

    /// Wrap raw fixed-size bytes.
    pub fn from_bytes(bytes: [u8; 64]) -> Self {
        Self(bytes)
    }

    /// Raw signature bytes.
    pub fn as_bytes(&self) -> &[u8; 64] {
        &self.0
    }

    /// Owned byte copy (storage boundary).
    pub fn to_vec(&self) -> Vec<u8> {
        self.0.to_vec()
    }

    /// Lowercase hex encoding (deterministic, no randomness).
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }

    /// Parse a lowercase hex string of exactly 128 characters.
    pub fn from_hex(input: &str) -> Result<Self, String> {
        let decoded =
            hex::decode(input).map_err(|e| format!("invalid Ed25519 signature hex: {e}"))?;
        Self::try_from(decoded)
    }
}

impl TryFrom<Vec<u8>> for Ed25519CertificateSignature {
    type Error = String;

    fn try_from(value: Vec<u8>) -> Result<Self, Self::Error> {
        let bytes: [u8; 64] = value.try_into().map_err(|v: Vec<u8>| {
            format!(
                "invalid Ed25519 signature length: expected {} bytes, got {}",
                Self::LEN,
                v.len()
            )
        })?;
        Ok(Self(bytes))
    }
}

impl TryFrom<&[u8]> for Ed25519CertificateSignature {
    type Error = String;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        Self::try_from(value.to_vec())
    }
}

impl fmt::Display for Ed25519CertificateSignature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl serde::Serialize for Ed25519CertificateSignature {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_hex())
    }
}

impl<'de> serde::Deserialize<'de> for Ed25519CertificateSignature {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Self::from_hex(&s).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Ed25519CertificateSignature {
        let mut bytes = [0u8; 64];
        for (i, b) in bytes.iter_mut().enumerate() {
            *b = i as u8;
        }
        Ed25519CertificateSignature(bytes)
    }

    #[test]
    fn roundtrip_hex() {
        let sig = sample();
        assert_eq!(sig.to_hex().len(), 128);
        assert_eq!(
            Ed25519CertificateSignature::from_hex(&sig.to_hex()).unwrap(),
            sig
        );
    }

    #[test]
    fn try_from_vec_enforces_64_bytes() {
        assert!(Ed25519CertificateSignature::try_from(vec![0u8; 64]).is_ok());
        assert!(Ed25519CertificateSignature::try_from(vec![0u8; 63]).is_err());
        assert!(Ed25519CertificateSignature::try_from(vec![0u8; 65]).is_err());
    }

    #[test]
    fn serde_roundtrip() {
        let sig = sample();
        let json = serde_json::to_string(&sig).unwrap();
        assert_eq!(json, format!("\"{}\"", sig.to_hex()));
        let back: Ed25519CertificateSignature = serde_json::from_str(&json).unwrap();
        assert_eq!(back, sig);
        assert!(serde_json::from_str::<Ed25519CertificateSignature>("\"abcd\"").is_err());
    }
}
