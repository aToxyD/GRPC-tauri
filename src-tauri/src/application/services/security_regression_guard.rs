//! Security/trust regression guard for restores (SEC-005 BR-03/BR-14).
//!
//! A restore must never silently regress monotonic security state. The guard
//! compares the backup candidate fingerprint against the live fingerprint and
//! classifies the restore as `Equal`, `Newer`, or `Regressing`. Regressing
//! restores require the distinct `RESTORE-OLDER-TRUST` operator ceremony on
//! top of the ordinary `RESTORE` confirmation.
//!
//! Monotonic security state (deliberately narrow):
//!   - maximum credential generation per `credential_id`;
//!   - the ACTIVE WILAYA trust-anchor generation;
//!   - the imported-package registry set;
//!   - per-issuer replay maxima;
//!   - the node's ACTIVE ADMIN credential (XB-B).
//!
//! Account administration fields (`deleted`, `role`, enabled/disabled) are
//! intentionally NOT monotonic and are excluded from the comparison.
//!
//! XB-B: a candidate that lacks the live node's ACTIVE ADMIN credential — a
//! pre-ADMIN backup, an older generation, or a different credential lineage —
//! would restore a `NoActiveAdmin`/stale-admin state and re-open the password
//! gate with obsolete credentials. Such candidates are classified as
//! regressing and require the `RESTORE-OLDER-TRUST` ceremony.
//!
//! The fiscal historical guard remains separate and unchanged; this guard does
//! not replace it.

use crate::domain::ports::backup::SecurityFingerprint;

/// Result of comparing a backup candidate fingerprint against the live one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestoreRegressionStatus {
    /// Candidate security state is identical to live.
    Equal,
    /// Candidate security state is equal or a superset (newer) — no regression.
    Newer,
    /// Candidate security state is older in at least one monotonic dimension.
    Regressing,
}

impl RestoreRegressionStatus {
    pub fn is_regressing(&self) -> bool {
        matches!(self, RestoreRegressionStatus::Regressing)
    }
}

pub struct SecurityRegressionGuard;

impl SecurityRegressionGuard {
    /// Compare the backup candidate fingerprint against the live fingerprint.
    ///
    /// Deterministic partial order: the candidate regresses iff ANY monotonic
    /// dimension is older in the candidate:
    ///   - some live credential has a higher max generation, or the same
    ///     generation but a higher terminal-status watermark, than the
    ///     candidate (or is absent from the candidate) — this makes both
    ///     pre-rotation and pre-revocation backups regressing;
    ///   - live has an ACTIVE WILAYA anchor older/absent in the candidate;
    ///   - some live imported package id is absent from the candidate;
    ///   - some live issuer sequence is higher than the candidate's;
    ///   - the candidate lacks the live node's ACTIVE ADMIN credential, holds
    ///     a different ADMIN credential, or holds an older ADMIN generation
    ///     (XB-B — stale-admin resurrection).
    pub fn compare(
        candidate: &SecurityFingerprint,
        live: &SecurityFingerprint,
    ) -> RestoreRegressionStatus {
        let mut regressing = false;

        for (credential_id, live_gen, live_watermark) in &live.credential_states {
            match candidate.credential_state(credential_id) {
                Some((cand_gen, cand_watermark))
                    if cand_gen > *live_gen
                        || (cand_gen == *live_gen && cand_watermark >= *live_watermark) => {}
                _ => regressing = true,
            }
        }

        // XB-B: the node's authoritative ACTIVE ADMIN credential must be
        // present in the candidate at the same credential_id and an equal or
        // newer generation. Missing/different/older ⇒ the restore would
        // resurrect a stale or pre-admin authentication state.
        if let Some((live_admin_id, live_admin_gen)) = live.active_admin_credential.as_ref() {
            match candidate.active_admin_credential.as_ref() {
                Some((cand_id, cand_gen))
                    if cand_id == live_admin_id && cand_gen >= live_admin_gen => {}
                _ => regressing = true,
            }
        }

        if let Some(live_anchor) = live.active_wilaya_anchor_generation {
            match candidate.active_wilaya_anchor_generation {
                Some(cand_anchor) if cand_anchor >= live_anchor => {}
                _ => regressing = true,
            }
        }

        for package_id in &live.registry_package_ids {
            if !candidate.registry_package_ids.contains(package_id) {
                regressing = true;
            }
        }

        for (issuer, live_seq) in &live.issuer_sequences {
            match candidate.sequence_for(issuer) {
                Some(cand_seq) if cand_seq >= *live_seq => {}
                _ => regressing = true,
            }
        }

        // SEC-054 (F1): protect the producer transport stream. A backup whose
        // `(issuer, target)` producer sequence trails the live stream, or that is
        // missing it entirely, must be treated as a regression (never `Equal`),
        // otherwise a matching-but-older backup could rewind the producer sequence
        // and later re-issue a duplicate outbound sequence. The consumer receptions
        // already reject such a duplicate as replay (availability impact), but
        // restore-time prevention removes the fault at its source.
        for (issuer, target, live_seq) in &live.transport_sequences {
            let trailing = match candidate.transport_sequence_for(issuer, target) {
                Some(cand_seq) => cand_seq < *live_seq,
                None => true,
            };
            if trailing {
                regressing = true;
            }
        }

        if regressing {
            RestoreRegressionStatus::Regressing
        } else if candidate == live {
            RestoreRegressionStatus::Equal
        } else {
            RestoreRegressionStatus::Newer
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ports::backup::SecurityFingerprint;

    fn fp(
        credentials: Vec<(&str, u64, u64)>,
        anchor: Option<u64>,
        registry: Vec<&str>,
        sequences: Vec<(&str, u64)>,
    ) -> SecurityFingerprint {
        SecurityFingerprint {
            credential_states: credentials
                .into_iter()
                .map(|(c, g, w)| (c.to_string(), g, w))
                .collect(),
            active_wilaya_anchor_generation: anchor,
            registry_package_ids: registry.into_iter().map(String::from).collect(),
            issuer_sequences: sequences
                .into_iter()
                .map(|(i, s)| (i.to_string(), s))
                .collect(),
            active_admin_credential: None,
            transport_sequences: vec![],
        }
    }

    fn fp_admin(
        credentials: Vec<(&str, u64, u64)>,
        admin: Option<(&str, u64)>,
    ) -> SecurityFingerprint {
        SecurityFingerprint {
            credential_states: credentials
                .into_iter()
                .map(|(c, g, w)| (c.to_string(), g, w))
                .collect(),
            active_wilaya_anchor_generation: None,
            registry_package_ids: vec![],
            issuer_sequences: vec![],
            active_admin_credential: admin.map(|(id, g)| (id.to_string(), g)),
            transport_sequences: vec![],
        }
    }

    #[test]
    fn identical_state_is_equal() {
        let a = fp(vec![("c1", 2, 0)], Some(2), vec!["p1"], vec![("iss", 3)]);
        assert_eq!(
            SecurityRegressionGuard::compare(&a, &a),
            RestoreRegressionStatus::Equal
        );
    }

    #[test]
    fn newer_state_is_newer() {
        let candidate = fp(
            vec![("c1", 3, 0)],
            Some(3),
            vec!["p1", "p2"],
            vec![("iss", 4)],
        );
        let live = fp(vec![("c1", 2, 0)], Some(2), vec!["p1"], vec![("iss", 3)]);
        assert_eq!(
            SecurityRegressionGuard::compare(&candidate, &live),
            RestoreRegressionStatus::Newer
        );
    }

    #[test]
    fn older_credential_generation_is_regressing() {
        let candidate = fp(vec![("c1", 1, 0)], Some(1), vec![], vec![]);
        let live = fp(vec![("c1", 2, 0)], Some(2), vec![], vec![]);
        assert_eq!(
            SecurityRegressionGuard::compare(&candidate, &live),
            RestoreRegressionStatus::Regressing
        );
    }

    #[test]
    fn pre_revocation_backup_is_regressing_at_same_generation() {
        let candidate = fp(vec![("c1", 1, 0)], Some(1), vec![], vec![]);
        let live = fp(vec![("c1", 1, 1)], None, vec![], vec![]);
        assert_eq!(
            SecurityRegressionGuard::compare(&candidate, &live),
            RestoreRegressionStatus::Regressing
        );
    }

    #[test]
    fn missing_live_credential_in_candidate_is_regressing() {
        let candidate = fp(vec![], None, vec![], vec![]);
        let live = fp(vec![("c1", 1, 0)], Some(1), vec![], vec![]);
        assert_eq!(
            SecurityRegressionGuard::compare(&candidate, &live),
            RestoreRegressionStatus::Regressing
        );
    }

    #[test]
    fn missing_live_anchor_in_candidate_is_regressing() {
        let candidate = fp(vec![("c1", 1, 0)], None, vec![], vec![]);
        let live = fp(vec![("c1", 1, 0)], Some(1), vec![], vec![]);
        assert_eq!(
            SecurityRegressionGuard::compare(&candidate, &live),
            RestoreRegressionStatus::Regressing
        );
    }

    #[test]
    fn missing_live_package_in_candidate_is_regressing() {
        let candidate = fp(vec![("c1", 1, 0)], Some(1), vec![], vec![]);
        let live = fp(vec![("c1", 1, 0)], Some(1), vec!["p1"], vec![]);
        assert_eq!(
            SecurityRegressionGuard::compare(&candidate, &live),
            RestoreRegressionStatus::Regressing
        );
    }

    #[test]
    fn lower_issuer_sequence_is_regressing() {
        let candidate = fp(vec![("c1", 1, 0)], Some(1), vec![], vec![("iss", 2)]);
        let live = fp(vec![("c1", 1, 0)], Some(1), vec![], vec![("iss", 3)]);
        assert_eq!(
            SecurityRegressionGuard::compare(&candidate, &live),
            RestoreRegressionStatus::Regressing
        );
    }

    #[test]
    fn account_fields_are_not_monotonic() {
        // Role/deleted/enabled changes must NOT classify a restore as regressing:
        // the fingerprints carry only the monotonic dimensions.
        let candidate = fp(vec![("c1", 1, 0)], Some(1), vec![], vec![]);
        let live = fp(vec![("c1", 1, 0)], Some(1), vec![], vec![]);
        assert_eq!(
            SecurityRegressionGuard::compare(&candidate, &live),
            RestoreRegressionStatus::Equal
        );
    }

    // ── XB-B: ACTIVE ADMIN credential dimension ─────────────────────────────

    #[test]
    fn equal_admin_state_is_equal() {
        // Same ACTIVE ADMIN credential, same generation — never regressing.
        let candidate = fp_admin(vec![("c1", 1, 0), ("adm", 1, 0)], Some(("adm", 1)));
        let live = fp_admin(vec![("c1", 1, 0), ("adm", 1, 0)], Some(("adm", 1)));
        assert_eq!(
            SecurityRegressionGuard::compare(&candidate, &live),
            RestoreRegressionStatus::Equal
        );
    }

    #[test]
    fn newer_admin_generation_is_newer() {
        let candidate = fp_admin(vec![("adm", 2, 0)], Some(("adm", 2)));
        let live = fp_admin(vec![("adm", 1, 0)], Some(("adm", 1)));
        assert_eq!(
            SecurityRegressionGuard::compare(&candidate, &live),
            RestoreRegressionStatus::Newer
        );
    }

    #[test]
    fn backup_predating_admin_credential_is_regressing() {
        // Live has a usable ACTIVE ADMIN; the candidate (pre-ADMIN backup) has none.
        let candidate = fp_admin(vec![("c1", 1, 0)], None);
        let live = fp_admin(vec![("c1", 1, 0), ("adm", 1, 0)], Some(("adm", 1)));
        assert_eq!(
            SecurityRegressionGuard::compare(&candidate, &live),
            RestoreRegressionStatus::Regressing
        );
    }

    #[test]
    fn older_admin_generation_is_regressing() {
        let candidate = fp_admin(vec![("adm", 1, 0)], Some(("adm", 1)));
        let live = fp_admin(vec![("adm", 2, 0)], Some(("adm", 2)));
        assert_eq!(
            SecurityRegressionGuard::compare(&candidate, &live),
            RestoreRegressionStatus::Regressing
        );
    }

    #[test]
    fn different_admin_credential_is_regressing() {
        // A different ADMIN credential lineage must not silently replace the
        // authoritative one (fail-closed: explicit ceremony required).
        let candidate = fp_admin(vec![("adm2", 1, 0)], Some(("adm2", 1)));
        let live = fp_admin(vec![("adm1", 1, 0)], Some(("adm1", 1)));
        assert_eq!(
            SecurityRegressionGuard::compare(&candidate, &live),
            RestoreRegressionStatus::Regressing
        );
    }

    #[test]
    fn admin_dimension_is_neutral_when_live_has_no_admin() {
        // Unprovisioned / UNIT-style live state: no authoritative ADMIN to
        // protect — the dimension must never classify as regressing.
        let candidate = fp_admin(vec![], None);
        let live = fp_admin(vec![], None);
        assert_eq!(
            SecurityRegressionGuard::compare(&candidate, &live),
            RestoreRegressionStatus::Equal
        );
        // Candidate with an admin while live has none is merely "newer".
        let candidate = fp_admin(vec![("adm", 1, 0)], Some(("adm", 1)));
        assert_eq!(
            SecurityRegressionGuard::compare(&candidate, &live),
            RestoreRegressionStatus::Newer
        );
    }

    // ── SEC-054 (F1): producer transport sequence dimension ────────────────

    fn fp_transport(
        credentials: Vec<(&str, u64, u64)>,
        anchor: Option<u64>,
        registry: Vec<&str>,
        sequences: Vec<(&str, u64)>,
        transport: Vec<(&str, &str, u64)>,
    ) -> SecurityFingerprint {
        SecurityFingerprint {
            credential_states: credentials
                .into_iter()
                .map(|(c, g, w)| (c.to_string(), g, w))
                .collect(),
            active_wilaya_anchor_generation: anchor,
            registry_package_ids: registry.into_iter().map(String::from).collect(),
            issuer_sequences: sequences
                .into_iter()
                .map(|(i, s)| (i.to_string(), s))
                .collect(),
            active_admin_credential: None,
            transport_sequences: transport
                .into_iter()
                .map(|(i, t, s)| (i.to_string(), t.to_string(), s))
                .collect(),
        }
    }

    #[test]
    fn lower_producer_sequence_is_regressing() {
        // TEST 1 (Phase 4 / SEC-054): live producer sequence = 10, backup = 5,
        // all other dimensions equal ⇒ MUST NOT be `Equal`.
        let candidate = fp_transport(
            vec![("c1", 1, 0)],
            Some(1),
            vec!["p1"],
            vec![("iss", 3)],
            vec![("iss", "TGT", 5)],
        );
        let live = fp_transport(
            vec![("c1", 1, 0)],
            Some(1),
            vec!["p1"],
            vec![("iss", 3)],
            vec![("iss", "TGT", 10)],
        );
        assert_eq!(
            SecurityRegressionGuard::compare(&candidate, &live),
            RestoreRegressionStatus::Regressing
        );
    }

    #[test]
    fn missing_producer_stream_in_candidate_is_regressing() {
        // A backup that predates (or lacks) a live producer stream must not be
        // accepted silently — it would rewind that stream to zero.
        let candidate =
            fp_transport(vec![("c1", 1, 0)], Some(1), vec!["p1"], vec![("iss", 3)], vec![]);
        let live = fp_transport(
            vec![("c1", 1, 0)],
            Some(1),
            vec!["p1"],
            vec![("iss", 3)],
            vec![("iss", "TGT", 10)],
        );
        assert_eq!(
            SecurityRegressionGuard::compare(&candidate, &live),
            RestoreRegressionStatus::Regressing
        );
    }

    #[test]
    fn equal_producer_sequence_is_equal() {
        // TEST 2 (Phase 4 / SEC-054): live = 10, backup = 10 ⇒ no false regression.
        let a = fp_transport(
            vec![("c1", 1, 0)],
            Some(1),
            vec!["p1"],
            vec![("iss", 3)],
            vec![("iss", "TGT", 10)],
        );
        assert_eq!(
            SecurityRegressionGuard::compare(&a, &a),
            RestoreRegressionStatus::Equal
        );
    }

    #[test]
    fn higher_producer_sequence_is_newer() {
        // TEST 3 (Phase 4 / SEC-054): live = 10, backup = 15 ⇒ preserves the
        // existing "forward state" semantics (Newer, no regression ceremony).
        let candidate = fp_transport(
            vec![("c1", 2, 0)],
            Some(2),
            vec!["p1", "p2"],
            vec![("iss", 4)],
            vec![("iss", "TGT", 15)],
        );
        let live = fp_transport(
            vec![("c1", 1, 0)],
            Some(1),
            vec!["p1"],
            vec![("iss", 3)],
            vec![("iss", "TGT", 10)],
        );
        assert_eq!(
            SecurityRegressionGuard::compare(&candidate, &live),
            RestoreRegressionStatus::Newer
        );
    }

    #[test]
    fn producer_dimension_is_neutral_when_live_has_no_stream() {
        // Unprovisioned producer state: nothing to protect — the dimension must
        // never introduce a regression on its own.
        let a = fp_transport(vec![], None, vec![], vec![], vec![]);
        assert_eq!(
            SecurityRegressionGuard::compare(&a, &a),
            RestoreRegressionStatus::Equal
        );
        // Candidate carrying a producer stream while live has none is merely
        // "newer" (never a regression).
        let candidate = fp_transport(vec![], None, vec![], vec![], vec![("iss", "TGT", 3)]);
        let live = fp_transport(vec![], None, vec![], vec![], vec![]);
        assert_eq!(
            SecurityRegressionGuard::compare(&candidate, &live),
            RestoreRegressionStatus::Newer
        );
    }
}
