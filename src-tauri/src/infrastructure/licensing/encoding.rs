//! Base64url encoding helpers (artifact-spec §2: `signature` and `public_key`
//! are base64url; ADR-0042 §4: `subject.id` is base64url(URL_SAFE_NO_PAD)).

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;

/// Errors produced by base64url decoding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodingError(pub String);

impl std::fmt::Display for EncodingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "base64url: {}", self.0)
    }
}

impl std::error::Error for EncodingError {}

/// Encodes bytes as base64url (no padding).
pub fn encode_base64url(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

/// Decodes a base64url string (no padding). Padded/standard base64 is rejected.
pub fn decode_base64url(input: &str) -> Result<Vec<u8>, EncodingError> {
    URL_SAFE_NO_PAD
        .decode(input)
        .map_err(|e| EncodingError(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let bytes = vec![0u8, 1, 2, 250, 255];
        let encoded = encode_base64url(&bytes);
        assert_eq!(decode_base64url(&encoded).unwrap(), bytes);
    }

    #[test]
    fn standard_base64_padding_is_rejected() {
        assert!(decode_base64url("AA==").is_err());
    }

    #[test]
    fn empty_round_trip() {
        let encoded = encode_base64url(&[]);
        assert_eq!(decode_base64url(&encoded).unwrap(), Vec::<u8>::new());
    }
}
