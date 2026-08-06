//! Challenge–Response protocol model.
//!
//! RFC 2026-08-04-node-identity-trust §3.7 / ADR-0038 §5 / ADR-0039 §5.
//!
//! The challenge is a fixed-structure message with NO timestamps; replay
//! protection comes from the one-shot state machine (`ChallengeState`), not
//! from session expiry. `canonical_bytes()` is the single source of truth for
//! the signed representation.

use serde::{Deserialize, Serialize};

use crate::domain::identity::canonical;

/// Protocol version of the challenge message.
pub const CHALLENGE_PROTOCOL_VERSION: u16 = 1;

/// Challenge scheme version — bumped only when the message semantics change.
pub const CHALLENGE_VERSION: u16 = 1;

/// A server-issued, one-shot challenge.
///
/// Fields: `protocol_version`, `session_id` (fresh, no timestamps),
/// `node_identity_id` (target node), `nonce` (32 random bytes), `challenge_version`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChallengeMessage {
    pub protocol_version: u16,
    pub session_id: uuid::Uuid,
    pub node_identity_id: uuid::Uuid,
    pub nonce: [u8; 32],
    pub challenge_version: u16,
}

impl ChallengeMessage {
    /// Build a fresh challenge bound to the target node identity.
    pub fn new(session_id: uuid::Uuid, node_identity_id: uuid::Uuid, nonce: [u8; 32]) -> Self {
        Self {
            protocol_version: CHALLENGE_PROTOCOL_VERSION,
            session_id,
            node_identity_id,
            nonce,
            challenge_version: CHALLENGE_VERSION,
        }
    }

    /// Canonical signed representation (ADR-0039 §5) — the single source of
    /// truth for signing/verification. Deterministic, no wall clock, no
    /// randomness at verification.
    ///
    /// Layout (v1): [CANONICAL_ENCODING_VERSION u16 BE][protocol_version u16 BE]
    /// [session_id 16][node_identity_id 16][nonce 32][challenge_version u16 BE] = 70 bytes.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(70);
        canonical::write_u16(&mut out, canonical::CANONICAL_ENCODING_VERSION);
        canonical::write_u16(&mut out, self.protocol_version);
        canonical::write_uuid(&mut out, &self.session_id);
        canonical::write_uuid(&mut out, &self.node_identity_id);
        out.extend_from_slice(&self.nonce);
        canonical::write_u16(&mut out, self.challenge_version);
        out
    }
}

/// Explicit per-challenge lifecycle state (B3 design).
///
/// A single map of `(ChallengeMessage, ChallengeState)` prevents impossible
/// states (a challenge is never both pending and consumed) and allows future
/// states (e.g. `Invalidated`) without redesign.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChallengeState {
    /// Challenge issued, completion not yet attempted.
    Pending,
    /// First completion attempt received — fail-closed replay protection.
    /// The challenge can never be used again, success or failure.
    Consumed,
}

/// In-memory one-shot challenge store (no wall clock, no persistence).
#[derive(Debug, Default)]
pub struct IdentityChallengeState {
    challenges: std::collections::HashMap<uuid::Uuid, (ChallengeMessage, ChallengeState)>,
}

impl IdentityChallengeState {
    /// Register a fresh challenge as `Pending`.
    pub fn begin(&mut self, challenge: ChallengeMessage) {
        self.challenges
            .insert(challenge.session_id, (challenge, ChallengeState::Pending));
    }

    /// Atomically consume a `Pending` challenge for the first completion attempt.
    ///
    /// Fail-closed (ADR-0039 / B3): the challenge is flipped to `Consumed` on
    /// the FIRST attempt regardless of outcome, so a replayed or re-submitted
    /// message is always rejected — even after a failed passphrase attempt.
    pub fn consume(&mut self, session_id: &uuid::Uuid) -> Option<ChallengeMessage> {
        if let Some((challenge, state)) = self.challenges.get_mut(session_id) {
            if *state == ChallengeState::Pending {
                *state = ChallengeState::Consumed;
                return Some(challenge.clone());
            }
        }
        None
    }

    /// Observe the current lifecycle state of a challenge (tests / audit).
    pub fn state(&self, session_id: &uuid::Uuid) -> Option<ChallengeState> {
        self.challenges.get(session_id).map(|(_, s)| *s)
    }

    /// Number of tracked challenges (memory bound / diagnostics).
    pub fn len(&self) -> usize {
        self.challenges.len()
    }

    /// True when no challenges are tracked.
    pub fn is_empty(&self) -> bool {
        self.challenges.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_challenge() -> ChallengeMessage {
        ChallengeMessage::new(
            uuid::Uuid::new_v4(),
            uuid::Uuid::new_v4(),
            [7u8; 32],
        )
    }

    #[test]
    fn canonical_bytes_layout_v1() {
        let challenge = sample_challenge();
        let bytes = challenge.canonical_bytes();
        assert_eq!(bytes.len(), 70);
        // CANONICAL_ENCODING_VERSION = 1, protocol_version = 1 (both BE).
        assert_eq!(&bytes[0..2], &[0, 1]);
        assert_eq!(&bytes[2..4], &[0, 1]);
        // session_id at 4..20, node_identity_id at 20..36.
        assert_eq!(&bytes[4..20], challenge.session_id.as_bytes());
        assert_eq!(&bytes[20..36], challenge.node_identity_id.as_bytes());
        assert_eq!(&bytes[36..68], &challenge.nonce);
        assert_eq!(&bytes[68..70], &[0, 1]);
    }

    #[test]
    fn canonical_bytes_are_deterministic() {
        let challenge = sample_challenge();
        assert_eq!(challenge.canonical_bytes(), challenge.canonical_bytes());
    }

    #[test]
    fn challenge_is_consumed_atomically_on_first_attempt() {
        let mut state = IdentityChallengeState::default();
        let challenge = sample_challenge();
        state.begin(challenge.clone());
        let session_id = challenge.session_id;

        assert_eq!(state.state(&session_id), Some(ChallengeState::Pending));
        assert_eq!(state.consume(&session_id), Some(challenge.clone()));
        assert_eq!(state.state(&session_id), Some(ChallengeState::Consumed));

        // Replay / retry after a failure is rejected.
        assert_eq!(state.consume(&session_id), None);
        assert_eq!(state.state(&session_id), Some(ChallengeState::Consumed));
    }

    #[test]
    fn unknown_challenge_is_rejected() {
        let mut state = IdentityChallengeState::default();
        assert_eq!(state.consume(&uuid::Uuid::new_v4()), None);
    }

    #[test]
    fn new_session_cannot_consume_another() {
        let mut state = IdentityChallengeState::default();
        let first = sample_challenge();
        let second = sample_challenge();
        state.begin(first.clone());
        state.begin(second.clone());

        assert_eq!(state.consume(&first.session_id), Some(first.clone()));
        assert_eq!(state.consume(&first.session_id), None);
        // second is independent and still consumable once.
        assert_eq!(state.consume(&second.session_id), Some(second));
    }
}
