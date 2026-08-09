//! Credential Guard — per-credential generation monotonicity (RFC 2026-08-04 §3.4.2).
//!
//! Pure, deterministic lifecycle check keyed on `(credential_id, generation)`.
//! Only strictly increasing generations are accepted per credential:
//!
//! ```text
//! incoming.generation > stored_generation(credential_id)
//! ```
//!
//! Rejects `(X,2) → (X,1)` regardless of transport order (e.g. a perfectly ordered package
//! that would roll back a credential).
//!
//! Independence constraint (RFC §3.4.4): the Credential Guard MUST NOT depend on package
//! ordering. The guard signature therefore accepts no sequence input.
//!
//! Lifecycle semantics (RFC §3.3):
//! - `Rotate`   = same `credential_id` + `generation++` + new public key.
//! - `Re-Issue` = new `credential_id` + `generation = 1`.

/// Verdict of the Credential Guard for one incoming credential state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CredentialVerdict {
    /// `incoming.generation > stored_generation` — new authoritative state.
    Accept {
        credential_id: String,
        generation: u64,
    },
    /// `incoming.generation == stored_generation` — duplicate already-known state.
    /// The pipeline may treat this as an idempotent no-op.
    Replay {
        credential_id: String,
        generation: u64,
    },
    /// `incoming.generation < stored_generation` — regression. Reject.
    Rollback {
        credential_id: String,
        stored: u64,
        incoming: u64,
    },
    /// `incoming.generation == 0` — impossible generation. Reject fail-closed.
    RejectZero { credential_id: String },
}

/// Evaluates credential lifecycle monotonicity per credential.
pub struct CredentialGuard;

impl CredentialGuard {
    /// Evaluate an incoming `generation` for `credential_id` against the stored
    /// generation (`None` when the credential is new to this node).
    pub fn check(
        credential_id: &str,
        incoming_generation: u64,
        stored_generation: Option<u64>,
    ) -> CredentialVerdict {
        let id = credential_id.to_string();
        if incoming_generation == 0 {
            return CredentialVerdict::RejectZero { credential_id: id };
        }
        match stored_generation {
            None => CredentialVerdict::Accept {
                credential_id: id,
                generation: incoming_generation,
            },
            Some(stored) => {
                if incoming_generation > stored {
                    CredentialVerdict::Accept {
                        credential_id: id,
                        generation: incoming_generation,
                    }
                } else if incoming_generation == stored {
                    CredentialVerdict::Replay {
                        credential_id: id,
                        generation: incoming_generation,
                    }
                } else {
                    CredentialVerdict::Rollback {
                        credential_id: id,
                        stored,
                        incoming: incoming_generation,
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::transport_guard::{TransportGuard, TransportVerdict};
    use super::*;

    #[test]
    fn new_credential_accepted_from_generation_one() {
        assert_eq!(
            CredentialGuard::check("credential-a", 1, None),
            CredentialVerdict::Accept {
                credential_id: "credential-a".to_string(),
                generation: 1,
            }
        );
    }

    #[test]
    fn rotation_with_strictly_increasing_generation_accepted() {
        assert_eq!(
            CredentialGuard::check("credential-a", 2, Some(1)),
            CredentialVerdict::Accept {
                credential_id: "credential-a".to_string(),
                generation: 2,
            }
        );
        assert_eq!(
            CredentialGuard::check("credential-a", 9, Some(8)),
            CredentialVerdict::Accept {
                credential_id: "credential-a".to_string(),
                generation: 9,
            }
        );
    }

    #[test]
    fn reissue_with_fresh_credential_id_accepted() {
        // Re-Issue = new credential_id + generation = 1.
        assert_eq!(
            CredentialGuard::check("credential-b", 1, None),
            CredentialVerdict::Accept {
                credential_id: "credential-b".to_string(),
                generation: 1,
            }
        );
    }

    #[test]
    fn replay_of_same_generation_reported() {
        assert_eq!(
            CredentialGuard::check("credential-a", 3, Some(3)),
            CredentialVerdict::Replay {
                credential_id: "credential-a".to_string(),
                generation: 3,
            }
        );
    }

    #[test]
    fn rollback_rejected() {
        assert_eq!(
            CredentialGuard::check("credential-a", 1, Some(2)),
            CredentialVerdict::Rollback {
                credential_id: "credential-a".to_string(),
                stored: 2,
                incoming: 1,
            }
        );
    }

    #[test]
    fn zero_generation_rejected_fail_closed() {
        assert_eq!(
            CredentialGuard::check("credential-a", 0, None),
            CredentialVerdict::RejectZero {
                credential_id: "credential-a".to_string(),
            }
        );
        assert_eq!(
            CredentialGuard::check("credential-a", 0, Some(5)),
            CredentialVerdict::RejectZero {
                credential_id: "credential-a".to_string(),
            }
        );
    }

    #[test]
    fn rfc_342_example_rollback_rejected_despite_valid_transport() {
        // Package 402 arrives in perfect transport order (last applied = 401) but carries
        // Credential X Generation 8 while the stored generation is 9. The Credential Guard
        // rejects it even though the Transport Guard accepts the ordering.
        let transport = TransportGuard::check("issuer-a", 402, Some(401));
        assert!(matches!(transport, TransportVerdict::Accept { .. }));

        let credential = CredentialGuard::check("credential-x", 8, Some(9));
        assert_eq!(
            credential,
            CredentialVerdict::Rollback {
                credential_id: "credential-x".to_string(),
                stored: 9,
                incoming: 8,
            }
        );
    }

    #[test]
    fn credential_guard_is_independent_of_package_ordering() {
        // The verdict for a credential must not depend on any sequence context. The same
        // stored/generation pair always yields the same verdict.
        let fresh = CredentialGuard::check("credential-c", 1, None);
        for _ in 0..3 {
            assert_eq!(CredentialGuard::check("credential-c", 1, None), fresh);
        }
        let rotated = CredentialGuard::check("credential-c", 5, Some(4));
        for _ in 0..3 {
            assert_eq!(CredentialGuard::check("credential-c", 5, Some(4)), rotated);
        }
    }
}
