//! Identity bootstrap state machine (B5).
//!
//! RFC 2026-08-04-node-identity-trust §3.6 / ADR-0038.
//!
//! `IdentityBootstrapState` is a PURE PROJECTION of the bootstrap workflow — it
//! is NEVER persisted. `IdentityBootstrapStatusService::compute()` derives it
//! from the Identity Store, the node key store, the `.adminkey`, and the
//! `users` table. One-way transitions are enforced by the bootstrap operation
//! guards (`can_transition_to` documents the graph; services/commands reject
//! any backward or out-of-order attempt).

use serde::{Deserialize, Serialize};

/// Explicit, derived bootstrap state for a node.
///
/// WILAYA chain:
/// - `Uninitialized` — no node key generated, no CSR exported.
/// - `WaitingForRootCertificate` — CSR exported, node secret persisted, awaiting
///   the offline Authority Root-signed WILAYA certificate.
/// - `WilayaActive` — Root-signed WILAYA certificate finalized + ACTIVE.
/// - `AdminProvisioned` — first ADMIN issued, `.adminkey` written, linked admin
///   user exists; a legacy password may still be active (additive window).
/// - `Ready` — fully bootstrapped: the linked admin user is identity-only
///   (empty password hash) so Challenge–Response is the sole credential.
///
/// UNIT chain (RFC §3.12, B6-A; never bundles the WILAYA anchor — strict
/// two-step install):
/// - `UnitWaitingForCertificate` — UNIT CSR exported, node secret persisted,
///   awaiting the WILAYA-signed UNIT certificate.
/// - `UnitActive` — WILAYA-signed UNIT certificate finalized + ACTIVE.
///
/// State is derived, never stored; `Ready` vs `AdminProvisioned` is the only
/// difference driven by the linked admin user's credential state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum IdentityBootstrapState {
    Uninitialized,
    WaitingForRootCertificate,
    WilayaActive,
    AdminProvisioned,
    Ready,
    UnitWaitingForCertificate,
    UnitActive,
}

impl IdentityBootstrapState {
    pub const fn as_str(&self) -> &'static str {
        match self {
            IdentityBootstrapState::Uninitialized => "UNINITIALIZED",
            IdentityBootstrapState::WaitingForRootCertificate => "WAITING_FOR_ROOT_CERTIFICATE",
            IdentityBootstrapState::WilayaActive => "WILAYA_ACTIVE",
            IdentityBootstrapState::AdminProvisioned => "ADMIN_PROVISIONED",
            IdentityBootstrapState::Ready => "READY",
            IdentityBootstrapState::UnitWaitingForCertificate => "UNIT_WAITING_FOR_CERTIFICATE",
            IdentityBootstrapState::UnitActive => "UNIT_ACTIVE",
        }
    }

    pub fn parse(s: &str) -> Option<IdentityBootstrapState> {
        match s {
            "UNINITIALIZED" => Some(IdentityBootstrapState::Uninitialized),
            "WAITING_FOR_ROOT_CERTIFICATE" => {
                Some(IdentityBootstrapState::WaitingForRootCertificate)
            }
            "WILAYA_ACTIVE" => Some(IdentityBootstrapState::WilayaActive),
            "ADMIN_PROVISIONED" => Some(IdentityBootstrapState::AdminProvisioned),
            "READY" => Some(IdentityBootstrapState::Ready),
            "UNIT_WAITING_FOR_CERTIFICATE" => {
                Some(IdentityBootstrapState::UnitWaitingForCertificate)
            }
            "UNIT_ACTIVE" => Some(IdentityBootstrapState::UnitActive),
            _ => None,
        }
    }

    /// One-way transition table (RFC §3.6). Transitions are strictly forward:
    ///
    /// ```text
    /// Uninitialized
    ///     │
    ///     ├── WILAYA chain ──────────────► WaitingForRootCertificate
    ///     │                                    │
    ///     │                                    ▼
    ///     │                                WilayaActive
    ///     │                                    │
    ///     │                                    ▼
    ///     │                               AdminProvisioned
    ///     │                                    │
    ///     │                                    ▼
    ///     │                                    Ready
    ///     │
    ///     └── UNIT chain ───────────────────► UnitWaitingForCertificate
    ///                                              │
    ///                                              ▼
    ///                                         UnitActive
    /// ```
    ///
    /// Any other pair — including all backward transitions and any WILAYA↔UNIT
    /// crossing — is rejected.
    pub const fn can_transition_to(&self, next: &IdentityBootstrapState) -> bool {
        matches!(
            (self, next),
            (
                IdentityBootstrapState::Uninitialized,
                IdentityBootstrapState::WaitingForRootCertificate
            ) | (
                IdentityBootstrapState::WaitingForRootCertificate,
                IdentityBootstrapState::WilayaActive
            ) | (
                IdentityBootstrapState::WilayaActive,
                IdentityBootstrapState::AdminProvisioned
            ) | (
                IdentityBootstrapState::AdminProvisioned,
                IdentityBootstrapState::Ready
            ) | (
                IdentityBootstrapState::Uninitialized,
                IdentityBootstrapState::UnitWaitingForCertificate
            ) | (
                IdentityBootstrapState::UnitWaitingForCertificate,
                IdentityBootstrapState::UnitActive
            )
        )
    }
}

impl std::fmt::Display for IdentityBootstrapState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forward_transitions_are_allowed() {
        let chain = [
            IdentityBootstrapState::Uninitialized,
            IdentityBootstrapState::WaitingForRootCertificate,
            IdentityBootstrapState::WilayaActive,
            IdentityBootstrapState::AdminProvisioned,
            IdentityBootstrapState::Ready,
        ];
        for pair in chain.windows(2) {
            assert!(
                pair[0].can_transition_to(&pair[1]),
                "{:?} -> {:?} must be allowed",
                pair[0],
                pair[1]
            );
        }
        let unit_chain = [
            IdentityBootstrapState::Uninitialized,
            IdentityBootstrapState::UnitWaitingForCertificate,
            IdentityBootstrapState::UnitActive,
        ];
        for pair in unit_chain.windows(2) {
            assert!(
                pair[0].can_transition_to(&pair[1]),
                "{:?} -> {:?} must be allowed",
                pair[0],
                pair[1]
            );
        }
    }

    #[test]
    fn backward_and_skipping_transitions_are_rejected() {
        let states = [
            IdentityBootstrapState::Uninitialized,
            IdentityBootstrapState::WaitingForRootCertificate,
            IdentityBootstrapState::WilayaActive,
            IdentityBootstrapState::AdminProvisioned,
            IdentityBootstrapState::Ready,
            IdentityBootstrapState::UnitWaitingForCertificate,
            IdentityBootstrapState::UnitActive,
        ];
        let allowed: [(IdentityBootstrapState, IdentityBootstrapState); 6] = [
            (
                IdentityBootstrapState::Uninitialized,
                IdentityBootstrapState::WaitingForRootCertificate,
            ),
            (
                IdentityBootstrapState::WaitingForRootCertificate,
                IdentityBootstrapState::WilayaActive,
            ),
            (
                IdentityBootstrapState::WilayaActive,
                IdentityBootstrapState::AdminProvisioned,
            ),
            (
                IdentityBootstrapState::AdminProvisioned,
                IdentityBootstrapState::Ready,
            ),
            (
                IdentityBootstrapState::Uninitialized,
                IdentityBootstrapState::UnitWaitingForCertificate,
            ),
            (
                IdentityBootstrapState::UnitWaitingForCertificate,
                IdentityBootstrapState::UnitActive,
            ),
        ];
        for from in states {
            for to in states {
                if allowed.contains(&(from, to)) {
                    continue;
                }
                assert!(
                    !from.can_transition_to(&to),
                    "{from:?} -> {to:?} must be rejected"
                );
            }
        }
    }

    #[test]
    fn state_roundtrip_via_str() {
        let states = [
            IdentityBootstrapState::Uninitialized,
            IdentityBootstrapState::WaitingForRootCertificate,
            IdentityBootstrapState::WilayaActive,
            IdentityBootstrapState::AdminProvisioned,
            IdentityBootstrapState::Ready,
            IdentityBootstrapState::UnitWaitingForCertificate,
            IdentityBootstrapState::UnitActive,
        ];
        for state in states {
            assert_eq!(IdentityBootstrapState::parse(state.as_str()), Some(state));
        }
        assert_eq!(IdentityBootstrapState::parse("ROGUE"), None);
    }
}
