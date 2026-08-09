//! Subject binding evaluation (ADR-0042 §4).
//!
//! `payload.subject.id` is the base64url(URL_SAFE_NO_PAD) encoding of the
//! node's raw 32-byte Ed25519 public key. Binding is evaluated byte-exact
//! against the node's derived public key; no other convention is accepted.
//! This module is shared by import (outcome `not-for-this-node`) and by the
//! enforcement gate (binding is a precondition for entitlement grants).

use crate::infrastructure::licensing::encoding::decode_base64url;

/// Result of evaluating a subject id against the node's public key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubjectBinding {
    /// `subject.id` decodes to exactly the node's 32-byte public key.
    Bound,
    /// `subject.id` is a valid 32-byte key but NOT this node's key
    /// (valid signature, different node — ADR-0042 §4 distinction).
    Mismatch,
    /// `subject.id` is not strict base64url(URL_SAFE_NO_PAD) of 32 bytes.
    InvalidSubject,
    /// No node key available — binding cannot be evaluated (fail-closed).
    NodeKeyMissing,
}

pub fn evaluate_subject(subject_id: &str, node_public_key: Option<&[u8]>) -> SubjectBinding {
    let Some(node_key) = node_public_key else {
        return SubjectBinding::NodeKeyMissing;
    };
    if node_key.len() != 32 {
        return SubjectBinding::NodeKeyMissing;
    }
    let Ok(decoded) = decode_base64url(subject_id) else {
        return SubjectBinding::InvalidSubject;
    };
    if decoded.len() != 32 {
        return SubjectBinding::InvalidSubject;
    }
    if decoded != node_key {
        return SubjectBinding::Mismatch;
    }
    SubjectBinding::Bound
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::licensing::encoding::encode_base64url;

    const NODE_SECRET: [u8; 32] = [42u8; 32];

    fn node_public_key() -> Vec<u8> {
        ed25519_dalek::SigningKey::from_bytes(&NODE_SECRET)
            .verifying_key()
            .to_bytes()
            .to_vec()
    }

    #[test]
    fn bound_when_subject_matches_node_key() {
        let subject = encode_base64url(&node_public_key());
        assert_eq!(
            evaluate_subject(&subject, Some(&node_public_key())),
            SubjectBinding::Bound
        );
    }

    #[test]
    fn mismatch_for_another_node_key() {
        let other = ed25519_dalek::SigningKey::from_bytes(&[7u8; 32])
            .verifying_key()
            .to_bytes()
            .to_vec();
        let subject = encode_base64url(&other);
        assert_eq!(
            evaluate_subject(&subject, Some(&node_public_key())),
            SubjectBinding::Mismatch
        );
    }

    #[test]
    fn invalid_subject_when_not_32_bytes() {
        assert_eq!(
            evaluate_subject(
                &encode_base64url(&[1u8, 2u8, 3u8]),
                Some(&node_public_key())
            ),
            SubjectBinding::InvalidSubject
        );
    }

    #[test]
    fn invalid_subject_when_not_base64url() {
        assert_eq!(
            evaluate_subject("###not-b64url###", Some(&node_public_key())),
            SubjectBinding::InvalidSubject
        );
    }

    #[test]
    fn node_key_missing_is_fail_closed() {
        let subject = encode_base64url(&node_public_key());
        assert_eq!(
            evaluate_subject(&subject, None),
            SubjectBinding::NodeKeyMissing
        );
        assert_eq!(
            evaluate_subject(&subject, Some(&[1u8, 2u8])),
            SubjectBinding::NodeKeyMissing
        );
    }

    #[test]
    fn padded_base64_is_rejected_strict() {
        // URL_SAFE_NO_PAD: padding must be rejected (strict decoding).
        let public_key = node_public_key();
        let mut b64 = encode_base64url(&public_key);
        if !public_key.len().is_multiple_of(3) {
            b64.push('=');
        }
        assert_eq!(
            evaluate_subject(&b64, Some(&public_key)),
            SubjectBinding::InvalidSubject
        );
    }
}
