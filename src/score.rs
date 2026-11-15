//! Score mapping: Reppo's 0–100 verdict into the Solv FICO range, with age
//! decay and tiers (docs/SPEC.md §4.2, §4.3).
//!
//! Everything here is a pure function. The mapping is a public promise
//! printed on every receipt; changing it requires stating the reason in the
//! PR (AGENTS.md hard prohibitions).

use serde::{Deserialize, Serialize};

/// Seconds in a day; the unit `age_days` is counted in.
const SECONDS_PER_DAY: u64 = 86_400;

/// The four pillars sent to Reppo as criteria (SPEC §4.1), scored 0–10.
///
/// The 0–10 range is trusted input from Reppo here; the acceptance point
/// that validates it is the Reppo worker that writes a `SettledScore`
/// (PR-09), not this type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pillars {
    pub reliability: u8,
    pub track_record: u8,
    pub safety: u8,
    pub maturity: u8,
}

/// A settled Reppo evaluation, as stored on a profile. Age is derived at
/// read time from `settled_at_unix`, so a score grows old honestly instead
/// of freezing at whatever age it was written with.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SettledScore {
    /// Reppo `verdict.score`, 0–100.
    pub reppo_score: f64,
    pub eval_id: String,
    pub settled_at_unix: u64,
    pub pillars: Pillars,
}

/// A score in Solv terms, computed at decision time. All four numbers
/// appear on the receipt (SPEC §4.2): showing only the final figure would
/// hide that an approval rests on stale evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EffectiveScore {
    pub fico_raw: u16,
    pub decay: u32,
    pub fico_effective: u16,
    pub age_days: u32,
}

/// The deterministic Reppo→FICO mapping (SPEC §4.2):
///
/// ```text
/// fico_raw = 300 + round(reppo_score × 5.5)
/// ```
///
/// Input is clamped to 0–100; rounding is half away from zero. Every row of
/// the SPEC table is pinned by a test. A NaN input fails closed to 300: it
/// must never surface as an out-of-range number on a receipt.
pub fn fico_raw(reppo_score: f64) -> u16 {
    if reppo_score.is_nan() {
        return 300;
    }
    let clamped = reppo_score.clamp(0.0, 100.0);
    (300.0 + (clamped * 5.5).round()) as u16
}

/// Age decay in FICO points (SPEC §4.2):
///
/// ```text
/// age_days ≤ 30        → 0
/// 30 < age_days ≤ 90   → 5 × ceil((age_days − 30) / 7)
/// age_days > 90        → None (the score is discarded, not lowered)
/// ```
pub fn decay(age_days: u32) -> Option<u32> {
    match age_days {
        0..=30 => Some(0),
        31..=90 => Some(5 * (age_days - 30).div_ceil(7)),
        _ => None,
    }
}

/// `fico_effective = max(300, fico_raw − decay)` (SPEC §4.2).
pub fn fico_effective(fico_raw: u16, decay: u32) -> u16 {
    let lowered = fico_raw as u32 - decay.min(fico_raw as u32);
    lowered.max(300) as u16
}

/// Computes the effective score for a settled evaluation at time `now`.
///
/// Returns `None` when the evidence is older than 90 days: the counterparty
/// is `UNRATED`, never quietly assigned a lowered number.
///
/// Accepted limit: a `settled_at` in the future (clock skew, or a tampered
/// snapshot — which SPEC §8.1 already declares untrusted for integrity)
/// reads as age 0 and keeps the score fresh for its full lifetime; future
/// timestamps are not treated as evidence of anything worse than skew.
pub fn effective_score(score: &SettledScore, now_unix: u64) -> Option<EffectiveScore> {
    let age_seconds = now_unix.saturating_sub(score.settled_at_unix);
    let age_days = (age_seconds / SECONDS_PER_DAY) as u32;
    let decay = decay(age_days)?;
    let raw = fico_raw(score.reppo_score);
    Some(EffectiveScore {
        fico_raw: raw,
        decay,
        fico_effective: fico_effective(raw, decay),
        age_days,
    })
}

/// Floor of the `FAIR` band (SPEC §4.3). `PolicyConfig::block_floor` must
/// equal this: the band that blocks and the score that blocks are the same
/// boundary, and two numbers that must agree but are configured apart are
/// a bug waiting to happen.
pub const FAIR_FLOOR: u16 = 580;
/// Floor of the `GOOD` band (SPEC §4.3).
pub const GOOD_FLOOR: u16 = 670;
/// Floor of the `VERY_GOOD` band (SPEC §4.3).
pub const VERY_GOOD_FLOOR: u16 = 740;
/// Floor of the `EXCEPTIONAL` band (SPEC §4.3).
pub const EXCEPTIONAL_FLOOR: u16 = 800;

/// Underwriting bands (SPEC §4.3). The four boundaries are the published
/// FICO band boundaries, so a reader can check them against a public
/// reference. They are *not* calibrated against outcomes, and SPEC §4.2
/// carries the words that must travel with them.
///
/// `Unrated` is a state, not a band: a counterparty nobody has assessed
/// gets no number, and inventing one is the subtlest form of a false
/// claim. It is never folded into `Fair`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Tier {
    Poor,
    Fair,
    Good,
    VeryGood,
    Exceptional,
    Unrated,
}

impl Tier {
    pub fn as_str(self) -> &'static str {
        match self {
            Tier::Poor => "POOR",
            Tier::Fair => "FAIR",
            Tier::Good => "GOOD",
            Tier::VeryGood => "VERY_GOOD",
            Tier::Exceptional => "EXCEPTIONAL",
            Tier::Unrated => "UNRATED",
        }
    }

    /// The band's default spending cap under `caps` (SPEC §4.3).
    ///
    /// Caps are operator-configurable, not hardcoded: whichever values were
    /// active when a decision was made are folded into `ruleset_version`
    /// and the evidence hash.
    ///
    /// `Exceptional` carries no cap of its own — it reads the operator's
    /// absolute ceiling, so the top band can never be configured above the
    /// hard limit `SOLV-LIMIT-002` enforces. The band raises its cap to
    /// meet that ceiling; it never removes one. No band is unrestricted.
    pub fn max_safe_spend_usd(self, caps: &Caps, absolute_cap_usd: f64) -> f64 {
        match self {
            Tier::Exceptional => absolute_cap_usd,
            Tier::VeryGood => caps.very_good_usd,
            Tier::Good => caps.good_usd,
            Tier::Fair => caps.fair_usd,
            Tier::Poor | Tier::Unrated => 0.0,
        }
    }
}

/// Default caps in USD per band (SPEC §4.3). Operator-configurable.
///
/// There is deliberately no `exceptional_usd`: that band reads
/// `absolute_cap_usd`. See [`Tier::max_safe_spend_usd`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Caps {
    pub fair_usd: f64,
    pub good_usd: f64,
    pub very_good_usd: f64,
}

impl Default for Caps {
    fn default() -> Self {
        Self {
            fair_usd: 5.00,
            good_usd: 50.00,
            very_good_usd: 250.00,
        }
    }
}

/// Derives the band from an effective score; `None` (no unexpired settled
/// score) means `UNRATED` (SPEC §4.2 "Cold start").
pub fn tier_for(fico_effective: Option<u16>) -> Tier {
    match fico_effective {
        Some(score) if score >= EXCEPTIONAL_FLOOR => Tier::Exceptional,
        Some(score) if score >= VERY_GOOD_FLOOR => Tier::VeryGood,
        Some(score) if score >= GOOD_FLOOR => Tier::Good,
        Some(score) if score >= FAIR_FLOOR => Tier::Fair,
        Some(_) => Tier::Poor,
        None => Tier::Unrated,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_table_rows_are_exact() {
        assert_eq!(fico_raw(0.0), 300);
        assert_eq!(fico_raw(50.0), 575);
        assert_eq!(fico_raw(78.0), 729);
        assert_eq!(fico_raw(100.0), 850);
    }

    #[test]
    fn nan_fails_closed_to_the_floor() {
        assert_eq!(fico_raw(f64::NAN), 300);
    }

    #[test]
    fn rounding_is_half_away_from_zero() {
        // 49 × 5.5 = 269.5 — the only class of input where the rounding
        // convention is observable.
        assert_eq!(fico_raw(49.0), 570);
    }

    #[test]
    fn tier_wire_strings_are_pinned() {
        assert_eq!(serde_json::to_string(&Tier::Poor).unwrap(), r#""POOR""#);
        assert_eq!(serde_json::to_string(&Tier::Fair).unwrap(), r#""FAIR""#);
        assert_eq!(serde_json::to_string(&Tier::Good).unwrap(), r#""GOOD""#);
        assert_eq!(
            serde_json::to_string(&Tier::VeryGood).unwrap(),
            r#""VERY_GOOD""#
        );
        assert_eq!(
            serde_json::to_string(&Tier::Exceptional).unwrap(),
            r#""EXCEPTIONAL""#
        );
        assert_eq!(
            serde_json::to_string(&Tier::Unrated).unwrap(),
            r#""UNRATED""#
        );
    }

    /// Every band boundary, from below and from on it. A band that is one
    /// point wide in the wrong place is the kind of error no reader of the
    /// table would catch.
    #[test]
    fn band_boundaries_are_exact_from_both_sides() {
        let cases = [
            (300, Tier::Poor),
            (579, Tier::Poor),
            (580, Tier::Fair),
            (669, Tier::Fair),
            (670, Tier::Good),
            (739, Tier::Good),
            (740, Tier::VeryGood),
            (799, Tier::VeryGood),
            (800, Tier::Exceptional),
            (850, Tier::Exceptional),
        ];
        for (score, expected) in cases {
            assert_eq!(tier_for(Some(score)), expected, "score {score}");
        }
        assert_eq!(tier_for(None), Tier::Unrated);
    }

    /// The same boundaries expressed in Reppo verdicts, which is how an
    /// operator actually experiences them (SPEC §4.3). `POOR` covering
    /// half the Reppo scale and `EXCEPTIONAL` needing 91 are deliberate,
    /// and pinning them here is what makes them deliberate rather than
    /// discovered in production.
    #[test]
    fn reppo_verdict_boundaries_are_pinned() {
        let band = |reppo: f64| tier_for(Some(fico_raw(reppo)));
        assert_eq!(band(50.0), Tier::Poor, "575");
        assert_eq!(band(51.0), Tier::Fair, "581");
        assert_eq!(band(67.0), Tier::Fair, "669");
        assert_eq!(band(68.0), Tier::Good, "674");
        assert_eq!(band(79.0), Tier::Good, "735");
        assert_eq!(band(80.0), Tier::VeryGood, "740");
        assert_eq!(band(90.0), Tier::VeryGood, "795");
        assert_eq!(band(91.0), Tier::Exceptional, "801");
    }

    #[test]
    fn caps_defaults_are_the_spec_table() {
        let caps = Caps::default();
        let absolute = 500.00;
        assert_eq!(Tier::Poor.max_safe_spend_usd(&caps, absolute), 0.00);
        assert_eq!(Tier::Fair.max_safe_spend_usd(&caps, absolute), 5.00);
        assert_eq!(Tier::Good.max_safe_spend_usd(&caps, absolute), 50.00);
        assert_eq!(Tier::VeryGood.max_safe_spend_usd(&caps, absolute), 250.00);
        assert_eq!(Tier::Unrated.max_safe_spend_usd(&caps, absolute), 0.00);
    }

    /// `EXCEPTIONAL` tracks the operator's ceiling instead of carrying its
    /// own number, so no configuration can lift the top band above the
    /// limit `SOLV-LIMIT-002` enforces.
    #[test]
    fn exceptional_never_exceeds_the_operator_ceiling() {
        let caps = Caps::default();
        assert_eq!(Tier::Exceptional.max_safe_spend_usd(&caps, 500.00), 500.00);
        assert_eq!(Tier::Exceptional.max_safe_spend_usd(&caps, 20.00), 20.00);
        assert_eq!(Tier::Exceptional.max_safe_spend_usd(&caps, 0.00), 0.00);
    }
}
