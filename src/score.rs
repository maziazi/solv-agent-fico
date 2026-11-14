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

/// Underwriting tiers (SPEC §4.3). `Unrated` is a tier, not an error: a
/// counterparty nobody has assessed gets no number, and inventing one is
/// the subtlest form of a false claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Tier {
    Prime,
    Standard,
    Subprime,
    HighRisk,
    Unrated,
}

impl Tier {
    pub fn as_str(self) -> &'static str {
        match self {
            Tier::Prime => "PRIME",
            Tier::Standard => "STANDARD",
            Tier::Subprime => "SUBPRIME",
            Tier::HighRisk => "HIGH_RISK",
            Tier::Unrated => "UNRATED",
        }
    }

    /// The tier's default spending cap under `caps` (SPEC §4.3).
    ///
    /// Caps are operator-configurable, not hardcoded: whichever values were
    /// active when a decision was made are folded into `ruleset_version`
    /// and the evidence hash (with PR-08).
    pub fn max_safe_spend_usd(self, caps: &Caps) -> f64 {
        match self {
            Tier::Prime => caps.prime_usd,
            Tier::Standard => caps.standard_usd,
            Tier::Subprime => caps.subprime_usd,
            Tier::HighRisk | Tier::Unrated => 0.0,
        }
    }
}

/// Default caps in USD per tier (SPEC §4.3). Operator-configurable.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Caps {
    pub prime_usd: f64,
    pub standard_usd: f64,
    pub subprime_usd: f64,
}

impl Default for Caps {
    fn default() -> Self {
        Self {
            prime_usd: 500.00,
            standard_usd: 100.00,
            subprime_usd: 10.00,
        }
    }
}

/// Derives the tier from an effective score; `None` (no unexpired settled
/// score) means `UNRATED` (SPEC §4.2 "Cold start").
pub fn tier_for(fico_effective: Option<u16>) -> Tier {
    match fico_effective {
        Some(score) if score >= 720 => Tier::Prime,
        Some(score) if score >= 640 => Tier::Standard,
        Some(score) if score >= 550 => Tier::Subprime,
        Some(_) => Tier::HighRisk,
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
        assert_eq!(serde_json::to_string(&Tier::Prime).unwrap(), r#""PRIME""#);
        assert_eq!(
            serde_json::to_string(&Tier::Standard).unwrap(),
            r#""STANDARD""#
        );
        assert_eq!(
            serde_json::to_string(&Tier::Subprime).unwrap(),
            r#""SUBPRIME""#
        );
        assert_eq!(
            serde_json::to_string(&Tier::HighRisk).unwrap(),
            r#""HIGH_RISK""#
        );
        assert_eq!(
            serde_json::to_string(&Tier::Unrated).unwrap(),
            r#""UNRATED""#
        );
    }
}
