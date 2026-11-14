//! Score-mapping tests (docs/SPEC.md §4.2, §4.3): every table row and every
//! decay branch is pinned here, because the mapping is a public promise
//! printed on every receipt.

use solv::score::{
    Caps, EffectiveScore, SettledScore, Tier, decay, effective_score, fico_effective, fico_raw,
    tier_for,
};
use solv::state::Profile;

const DAY: u64 = 86_400;

fn settled(reppo_score: f64, settled_at_unix: u64) -> SettledScore {
    SettledScore {
        reppo_score,
        eval_id: "ev_test".to_string(),
        settled_at_unix,
        pillars: solv::score::Pillars {
            reliability: 8,
            track_record: 7,
            safety: 9,
            maturity: 7,
        },
    }
}

#[test]
fn spec_mapping_table_is_exact() {
    assert_eq!(fico_raw(0.0), 300);
    assert_eq!(fico_raw(50.0), 575);
    assert_eq!(fico_raw(78.0), 729);
    assert_eq!(fico_raw(100.0), 850);
}

#[test]
fn mapping_is_monotonic_and_clamped() {
    let mut previous = fico_raw(0.0);
    for s in 1..=100 {
        let next = fico_raw(s as f64);
        assert!(next >= previous, "must not decrease at {s}");
        previous = next;
    }
    assert_eq!(fico_raw(-20.0), 300, "below-range input clamps");
    assert_eq!(fico_raw(150.0), 850, "above-range input clamps");
}

#[test]
fn decay_every_branch_and_edge() {
    assert_eq!(decay(0), Some(0));
    assert_eq!(decay(30), Some(0));
    assert_eq!(decay(31), Some(5));
    assert_eq!(decay(37), Some(5));
    assert_eq!(decay(38), Some(10));
    assert_eq!(decay(90), Some(45));
    assert_eq!(decay(91), None, "expired, not lowered");
    assert_eq!(decay(400), None);
}

#[test]
fn effective_score_floors_at_300() {
    assert_eq!(fico_effective(300, 45), 300);
    assert_eq!(fico_effective(410, 0), 410);
    assert_eq!(fico_effective(850, 45), 805);
}

#[test]
fn tier_boundaries_match_the_spec_table() {
    assert_eq!(tier_for(Some(720)), Tier::Prime);
    assert_eq!(tier_for(Some(850)), Tier::Prime);
    assert_eq!(tier_for(Some(719)), Tier::Standard);
    assert_eq!(tier_for(Some(640)), Tier::Standard);
    assert_eq!(tier_for(Some(639)), Tier::Subprime);
    assert_eq!(tier_for(Some(550)), Tier::Subprime);
    assert_eq!(tier_for(Some(549)), Tier::HighRisk);
    assert_eq!(tier_for(Some(300)), Tier::HighRisk);
    assert_eq!(tier_for(None), Tier::Unrated);
}

#[test]
fn default_caps_match_the_spec_table() {
    let caps = Caps::default();
    assert_eq!(caps.prime_usd, 500.00);
    assert_eq!(caps.standard_usd, 100.00);
    assert_eq!(caps.subprime_usd, 10.00);
    assert_eq!(Tier::Prime.max_safe_spend_usd(&caps), 500.00);
    assert_eq!(Tier::Standard.max_safe_spend_usd(&caps), 100.00);
    assert_eq!(Tier::Subprime.max_safe_spend_usd(&caps), 10.00);
    assert_eq!(Tier::HighRisk.max_safe_spend_usd(&caps), 0.00);
    assert_eq!(Tier::Unrated.max_safe_spend_usd(&caps), 0.00);
}

#[test]
fn operator_caps_override_defaults() {
    let caps = Caps {
        prime_usd: 250.0,
        standard_usd: 50.0,
        subprime_usd: 5.0,
    };
    assert_eq!(Tier::Prime.max_safe_spend_usd(&caps), 250.0);
    assert_eq!(Tier::HighRisk.max_safe_spend_usd(&caps), 0.0);
}

#[test]
fn effective_score_computes_age_from_settled_at() {
    let now = 100 * DAY;
    let score = settled(78.0, now - 3 * DAY);
    assert_eq!(
        effective_score(&score, now),
        Some(EffectiveScore {
            fico_raw: 729,
            decay: 0,
            fico_effective: 729,
            age_days: 3,
        })
    );
}

#[test]
fn effective_score_applies_decay_after_30_days() {
    let now = 100 * DAY;
    let score = settled(78.0, now - 45 * DAY);
    let computed = effective_score(&score, now).unwrap();
    assert_eq!(computed.age_days, 45);
    assert_eq!(computed.decay, 5 * (45u32 - 30).div_ceil(7));
    assert_eq!(computed.fico_effective as u32, 729 - computed.decay);
}

#[test]
fn effective_score_is_unrated_after_90_days() {
    let now = 100 * DAY;
    let score = settled(78.0, now - 91 * DAY);
    assert_eq!(effective_score(&score, now), None);
}

#[test]
fn profile_without_score_field_loads_from_old_snapshots() {
    // Snapshots written before the score field existed must keep loading.
    let legacy = serde_json::json!({
        "counterparty_name": "DataFeed Pro",
        "target_url": "https://api.example.com"
    });
    let profile: Profile = serde_json::from_value(legacy).unwrap();
    assert_eq!(profile.score, None);
    assert_eq!(profile.counterparty_name, "DataFeed Pro");
}

#[test]
fn profile_roundtrips_with_a_settled_score() {
    let profile = Profile {
        counterparty_name: "DataFeed Pro".to_string(),
        target_url: "https://api.example.com".to_string(),
        score: Some(settled(78.0, 100 * DAY)),
    };
    let bytes = serde_json::to_vec(&profile).unwrap();
    let reloaded: Profile = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(reloaded, profile);
}
