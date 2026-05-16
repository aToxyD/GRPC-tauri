use core::fmt::Write as _;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use subtle::ConstantTimeEq;

use crate::errors::{AppError, AppResult};
use crate::infrastructure::security::resolve_package_signing_key_32;

use super::{PackageSigner, PackageVerifier};

type HmacSha256 = Hmac<Sha256>;

#[derive(Clone, Copy, Debug, Default)]
pub struct HmacPackageSigner;

impl HmacPackageSigner {
    fn key() -> AppResult<[u8; 32]> {
        resolve_package_signing_key_32()
    }

    pub(crate) fn mac_bytes(payload: &[u8], key: &[u8; 32]) -> AppResult<[u8; 32]> {
        let mut mac = HmacSha256::new_from_slice(key)
            .map_err(|e| AppError::Internal(format!("HMAC signer init failed: {}", e)))?;
        mac.update(payload);
        Ok(mac.finalize().into_bytes().into())
    }

    fn sign_hex(payload: &[u8], key: &[u8; 32]) -> AppResult<String> {
        let bytes = Self::mac_bytes(payload, key)?;
        let mut out = String::with_capacity(bytes.len() * 2);
        for b in bytes {
            let _ = write!(&mut out, "{:02x}", b);
        }
        Ok(out)
    }
}

fn decode_hex_hmac_digest(signature: &str) -> AppResult<[u8; 32]> {
    let s = signature.trim();
    if s.len() != 64 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(AppError::Validation(
            crate::errors::ValidationError::InvalidFormat {
                field: "signature".into(),
                message: "توقيع الحزمة ليس توقيعًا سداسيًا بطول 32 بايت".into(),
            },
        ));
    }
    let mut out = [0u8; 32];
    for i in 0..32 {
        let byte = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).map_err(|_| {
            AppError::Validation(crate::errors::ValidationError::InvalidFormat {
                field: "signature".into(),
                message: "تعذّر فك ترميز التوقيع السداسي".into(),
            })
        })?;
        out[i] = byte;
    }
    Ok(out)
}

impl PackageSigner for HmacPackageSigner {
    fn sign(&self, plaintext: &[u8]) -> AppResult<String> {
        let key = Self::key()?;
        Self::sign_hex(plaintext, &key)
    }
}

impl PackageVerifier for HmacPackageSigner {
    fn verify(&self, plaintext: &[u8], signature: &str) -> AppResult<bool> {
        let key = Self::key()?;
        let expected = Self::mac_bytes(plaintext, &key)?;
        let provided = match decode_hex_hmac_digest(signature) {
            Ok(b) => b,
            Err(_) => return Ok(false),
        };
        Ok(bool::from(expected.ct_eq(&provided)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hmac_digest_compares_in_constant_time_and_rejects_mismatch() {
        let key = [7u8; 32];
        let payload = b"canonical-bytes-example";
        let expected = HmacPackageSigner::mac_bytes(payload, &key).unwrap();
        let matching = HmacPackageSigner::mac_bytes(payload, &key).unwrap();
        assert!(bool::from(expected.ct_eq(&matching)));

        let other_payload = HmacPackageSigner::mac_bytes(b"tampered", &key).unwrap();
        assert!(!bool::from(expected.ct_eq(&other_payload)));
    }

    #[test]
    fn hex_decode_accepts_only_64_hex_chars() {
        let r = decode_hex_hmac_digest("zz");
        assert!(r.is_err());
        let valid = "a".repeat(64);
        assert!(decode_hex_hmac_digest(&valid).is_ok());
    }
}
