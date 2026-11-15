//! Policy-gate tests (docs/SPEC.md §5.4): every row of the decision table,
//! the G1 fail-closed guarantee, G5 traceable reasons, binding row order,
//! and the escalation rules of §5.1.

use solv::policy::{
    DecisionKind, FiredRule, PolicyConfig, PreSpendRequest, RiskClass, RuleEffect, Tier1Signals,
    Tier2Outcome, Tier2Signals, Tier2Verdict, decide, needs_escalation,
};
use solv::reason::ReasonCode;
use solv::score::{SettledScore, Tier};
use solv::state::Profile;

const DAY: u64 = 86_400;
const NOW: u64 = 100 * DAY;

fn settled(reppo_score: f64, safety: u8, settled_days_ago: u64) -> SettledScore {
    SettledScore {
        reppo_score,
        eval_id: "ev_test".to_string(),
        settled_at_unix: NOW - settled_days_ago * DAY,
        pillars: solv::score::Pillars {
            reliability: 8,
            track_record: 7,
            safety,
            maturity: 7,
        },
    }
}

fn profile(score: Option<SettledScore>) -> Profile {
    Profile {
        counterparty_name: "DataFeed Pro".to_string(),
        target_url: "https://api.example.com".to_string(),
        score,
    }
}

fn prime_profile() -> Profile {
    profile(Some(settled(78.0, 9, 3)))
}

fn request(amount_usd: f64) -> PreSpendRequest {
    PreSpendRequest {
        counterparty_name: "DataFeed Pro".to_string(),
        target_url: "https://api.example.com/v1/feed".to_string(),
        amount_usd,
        service_description: "Real-time market data, 1 month access.".to_string(),
    }
}

fn tier1_clean() -> Tier1Signals {
    Tier1Signals {
        available: true,
        injection_probability_true: Some(0.02),
        risk_class: Some(RiskClass::None),
        risk_class_confidence: Some(0.97),
    }
}

fn run(
    request: &PreSpendRequest,
    profile: Option<&Profile>,
    tier1: &Tier1Signals,
    tier2: Option<&Tier2Signals>,
    fired_rules: &[FiredRule],
    elapsed_ms: u64,
) -> solv::policy::Decision {
    decide(
        request,
        profile,
        NOW,
        elapsed_ms,
        tier1,
        tier2,
        fired_rules,
        &PolicyConfig::default(),
    )
}

fn clean_case(amount: f64) -> solv::policy::Decision {
    let profile = prime_profile();
    run(
        &request(amount),
        Some(&profile),
        &tier1_clean(),
        None,
        &[],
        10,
    )
}

#[test]
fn row1_guard_refusal_blocks() {
    let profile = prime_profile();
    let tier2 = Tier2Signals {
        outcome: Tier2Outcome::GuardRefusal,
    };
    let decision = run(
        &request(50.0),
        Some(&profile),
        &tier1_clean(),
        Some(&tier2),
        &[],
        10,
    );
    assert_eq!(decision.kind, DecisionKind::Block);
    assert_eq!(decision.reasons[0].code, ReasonCode::Injection001);
    assert!(decision.satisfies_g5());
}

#[test]
fn row2_confident_tier1_injection_blocks() {
    let profile = prime_profile();
    let tier1 = Tier1Signals {
        available: true,
        injection_probability_true: Some(0.97),
        risk_class: Some(RiskClass::None),
        risk_class_confidence: Some(0.97),
    };
    let decision = run(&request(50.0), Some(&profile), &tier1, None, &[], 10);
    assert_eq!(decision.kind, DecisionKind::Block);
    assert_eq!(decision.reasons[0].code, ReasonCode::Injection002);
}

#[test]
fn row3_risk_class_exfiltration_blocks() {
    let profile = prime_profile();
    let tier1 = Tier1Signals {
        available: true,
        injection_probability_true: Some(0.02),
        risk_class: Some(RiskClass::Exfiltration),
        risk_class_confidence: Some(0.95),
    };
    let decision = run(&request(50.0), Some(&profile), &tier1, None, &[], 10);
    assert_eq!(decision.kind, DecisionKind::Block);
    assert_eq!(decision.reasons[0].code, ReasonCode::Injection003);
}

#[test]
fn row3_low_confidence_risk_class_does_not_block() {
    let profile = prime_profile();
    let tier1 = Tier1Signals {
        available: true,
        injection_probability_true: Some(0.02),
        risk_class: Some(RiskClass::Coercion),
        risk_class_confidence: Some(0.60),
    };
    let decision = run(&request(50.0), Some(&profile), &tier1, None, &[], 10);
    assert_eq!(decision.kind, DecisionKind::Approve);
}

#[test]
fn row4_learned_block_rule_blocks() {
    let profile = prime_profile();
    let rules = vec![FiredRule {
        rule_id: "014".to_string(),
        effect: RuleEffect::ForcedBlock,
    }];
    let decision = run(
        &request(50.0),
        Some(&profile),
        &tier1_clean(),
        None,
        &rules,
        10,
    );
    assert_eq!(decision.kind, DecisionKind::Block);
    assert_eq!(
        decision.reasons[0].code,
        ReasonCode::Rule("014".to_string())
    );
    assert_eq!(decision.reasons[0].code.as_str(), "SOLV-RULE-014");
}

#[test]
fn row5_low_safety_pillar_blocks() {
    // Track record is fine; safety alone forces the block (SPEC §4.1).
    let profile = profile(Some(settled(78.0, 3, 3)));
    let decision = run(
        &request(50.0),
        Some(&profile),
        &tier1_clean(),
        None,
        &[],
        10,
    );
    assert_eq!(decision.kind, DecisionKind::Block);
    assert_eq!(decision.reasons[0].code, ReasonCode::Score002);
}

#[test]
fn row6_score_below_block_floor_blocks() {
    let profile = profile(Some(settled(20.0, 9, 3)));
    let decision = run(
        &request(50.0),
        Some(&profile),
        &tier1_clean(),
        None,
        &[],
        10,
    );
    assert_eq!(decision.kind, DecisionKind::Block);
    assert_eq!(decision.reasons[0].code, ReasonCode::Score001);
}

#[test]
fn row7_all_tiers_down_fails_closed_to_hold() {
    let profile = prime_profile();
    let tier1 = Tier1Signals::unavailable();
    let decision = run(&request(50.0), Some(&profile), &tier1, None, &[], 10);
    assert_eq!(decision.kind, DecisionKind::Hold);
    assert_eq!(decision.reasons[0].code, ReasonCode::System003);
}

#[test]
fn row7_tier2_verdict_covers_a_tier1_outage() {
    let profile = prime_profile();
    let tier2 = Tier2Signals {
        outcome: Tier2Outcome::Verdict(Tier2Verdict::Approve),
    };
    let decision = run(
        &request(50.0),
        Some(&profile),
        &Tier1Signals::unavailable(),
        Some(&tier2),
        &[],
        10,
    );
    assert_eq!(decision.kind, DecisionKind::Approve);
}

#[test]
fn row8_latency_ceiling_holds() {
    let profile = prime_profile();
    let decision = run(
        &request(50.0),
        Some(&profile),
        &tier1_clean(),
        None,
        &[],
        2_001,
    );
    assert_eq!(decision.kind, DecisionKind::Hold);
    assert_eq!(decision.reasons[0].code, ReasonCode::System002);
}

#[test]
fn row9_cold_start_holds() {
    let no_score = profile(None);
    let decision = run(
        &request(50.0),
        Some(&no_score),
        &tier1_clean(),
        None,
        &[],
        10,
    );
    assert_eq!(decision.kind, DecisionKind::Hold);
    assert_eq!(decision.reasons[0].code, ReasonCode::Coldstart001);
    assert_eq!(decision.tier, Tier::Unrated);
}

#[test]
fn row9_expired_score_is_unrated_not_lowered() {
    let expired = profile(Some(settled(78.0, 9, 91)));
    let decision = run(
        &request(50.0),
        Some(&expired),
        &tier1_clean(),
        None,
        &[],
        10,
    );
    assert_eq!(decision.kind, DecisionKind::Hold);
    assert_eq!(decision.reasons[0].code, ReasonCode::Coldstart001);
    assert_eq!(decision.tier, Tier::Unrated);
}

#[test]
fn row10_absolute_cap_blocks() {
    let decision = clean_case(500.01);
    assert_eq!(decision.kind, DecisionKind::Block);
    assert_eq!(decision.reasons[0].code, ReasonCode::Limit002);
}

#[test]
fn row10_tier_cap_holds() {
    // 67 → 669 raw: STANDARD (cap 100), so 150 exceeds the tier cap while
    // staying under the absolute cap (500).
    let profile = profile(Some(settled(67.0, 9, 3))); // 300 + 368.5 → 669
    let decision = run(
        &request(150.0),
        Some(&profile),
        &tier1_clean(),
        None,
        &[],
        10,
    );
    assert_eq!(decision.tier, Tier::Standard);
    assert_eq!(decision.kind, DecisionKind::Hold);
    assert_eq!(decision.reasons[0].code, ReasonCode::Limit001);
}

#[test]
fn row11_tier2_revise_holds() {
    let profile = prime_profile();
    let tier2 = Tier2Signals {
        outcome: Tier2Outcome::Verdict(Tier2Verdict::Revise),
    };
    let decision = run(
        &request(50.0),
        Some(&profile),
        &tier1_clean(),
        Some(&tier2),
        &[],
        10,
    );
    assert_eq!(decision.kind, DecisionKind::Hold);
    assert_eq!(decision.reasons[0].code, ReasonCode::Reason001);
}

#[test]
fn row11_tier2_mismatch_holds() {
    let profile = prime_profile();
    let tier2 = Tier2Signals {
        outcome: Tier2Outcome::Verdict(Tier2Verdict::Mismatch),
    };
    let decision = run(
        &request(50.0),
        Some(&profile),
        &tier1_clean(),
        Some(&tier2),
        &[],
        10,
    );
    assert_eq!(decision.kind, DecisionKind::Hold);
    assert_eq!(decision.reasons[0].code, ReasonCode::Reason002);
}

#[test]
fn row12_clean_prime_approves_with_reasons() {
    let decision = clean_case(50.0);
    assert_eq!(decision.kind, DecisionKind::Approve);
    assert_eq!(decision.tier, Tier::Prime);
    assert_eq!(decision.max_safe_spend_usd, 500.0);
    assert_eq!(decision.reasons[0].code, ReasonCode::Score010);
    assert!(decision.reasons[0].message.contains("safety 9/10"));
    assert!(decision.satisfies_g5(), "APPROVE must be explained too");
}

#[test]
fn decay_companion_reason_attaches_when_score_aged() {
    let aged = profile(Some(settled(78.0, 9, 45)));
    let decision = run(&request(50.0), Some(&aged), &tier1_clean(), None, &[], 10);
    assert_eq!(decision.kind, DecisionKind::Approve);
    assert_eq!(decision.reasons.len(), 2);
    assert_eq!(decision.reasons[1].code, ReasonCode::Score003);
    assert_eq!(decision.reasons[1].effect, solv::reason::Effect::LoweredCap);
}

#[test]
fn injection_beats_cold_start_and_low_score() {
    // A hijack attempt is not rescued by missing data or a good score.
    let unrated = profile(None);
    let tier1 = Tier1Signals {
        available: true,
        injection_probability_true: Some(0.97),
        risk_class: Some(RiskClass::None),
        risk_class_confidence: Some(0.97),
    };
    let decision = run(&request(50.0), Some(&unrated), &tier1, None, &[], 10);
    assert_eq!(decision.kind, DecisionKind::Block);
    assert_eq!(decision.reasons[0].code, ReasonCode::Injection002);

    let low_score = profile(Some(settled(20.0, 9, 3)));
    let decision = run(&request(50.0), Some(&low_score), &tier1, None, &[], 10);
    assert_eq!(decision.reasons[0].code, ReasonCode::Injection002);
}

#[test]
fn safety_floor_outranks_score_floor() {
    // Both would block; row order puts safety first.
    let profile = profile(Some(settled(20.0, 2, 3)));
    let decision = run(
        &request(50.0),
        Some(&profile),
        &tier1_clean(),
        None,
        &[],
        10,
    );
    assert_eq!(decision.reasons[0].code, ReasonCode::Score002);
}

#[test]
fn limit_outranks_tier2_revise() {
    let profile = profile(Some(settled(67.0, 9, 3))); // STANDARD
    let tier2 = Tier2Signals {
        outcome: Tier2Outcome::Verdict(Tier2Verdict::Revise),
    };
    let decision = run(
        &request(150.0),
        Some(&profile),
        &tier1_clean(),
        Some(&tier2),
        &[],
        10,
    );
    assert_eq!(decision.kind, DecisionKind::Hold);
    assert_eq!(decision.reasons[0].code, ReasonCode::Limit001);
}

#[test]
fn forced_hold_rule_holds_a_clean_request() {
    let profile = prime_profile();
    let rules = vec![FiredRule {
        rule_id: "015".to_string(),
        effect: RuleEffect::ForcedHold,
    }];
    let decision = run(
        &request(50.0),
        Some(&profile),
        &tier1_clean(),
        None,
        &rules,
        10,
    );
    assert_eq!(decision.kind, DecisionKind::Hold);
    assert_eq!(decision.reasons[0].code.as_str(), "SOLV-RULE-015");
}

#[test]
fn nan_amount_fails_closed() {
    let decision = clean_case(f64::NAN);
    assert_eq!(decision.kind, DecisionKind::Block);
    assert_eq!(decision.reasons[0].code, ReasonCode::Limit002);
}

/// G1 (SPEC §3.3): with every tier unavailable, no input produces APPROVE —
/// and APPROVE is reachable only with an unexpired settled score and at
/// least one recorded tier verdict.
#[test]
fn g1_fail_closed_across_degraded_conditions() {
    let mut approve_count = 0;
    for profile in [
        profile(None),
        profile(Some(settled(78.0, 9, 3))),
        profile(Some(settled(20.0, 9, 3))),
        profile(Some(settled(78.0, 9, 91))),
    ] {
        for tier1 in [
            Tier1Signals::unavailable(),
            // Available but nothing parsed: not a verdict (G1 hole closed).
            Tier1Signals {
                available: true,
                injection_probability_true: None,
                risk_class: None,
                risk_class_confidence: None,
            },
            Tier1Signals {
                available: true,
                injection_probability_true: Some(0.02),
                risk_class: Some(RiskClass::None),
                risk_class_confidence: Some(0.97),
            },
        ] {
            for tier2 in [
                None,
                Some(Tier2Signals {
                    outcome: Tier2Outcome::Unavailable,
                }),
                Some(Tier2Signals {
                    outcome: Tier2Outcome::Verdict(Tier2Verdict::Approve),
                }),
                Some(Tier2Signals {
                    outcome: Tier2Outcome::Verdict(Tier2Verdict::Revise),
                }),
                Some(Tier2Signals {
                    outcome: Tier2Outcome::Verdict(Tier2Verdict::Mismatch),
                }),
                Some(Tier2Signals {
                    outcome: Tier2Outcome::GuardRefusal,
                }),
            ] {
                for rules in [
                    vec![],
                    vec![FiredRule {
                        rule_id: "014".to_string(),
                        effect: RuleEffect::ForcedBlock,
                    }],
                    vec![FiredRule {
                        rule_id: "015".to_string(),
                        effect: RuleEffect::ForcedHold,
                    }],
                ] {
                    for amount in [10.0, 600.0] {
                        let decision = run(
                            &request(amount),
                            Some(&profile),
                            &tier1,
                            tier2.as_ref(),
                            &rules,
                            10,
                        );
                        assert!(decision.satisfies_g5());
                        let tiers_live = tier1.has_verdict()
                            || matches!(
                                tier2.map(|t| t.outcome),
                                Some(Tier2Outcome::GuardRefusal) | Some(Tier2Outcome::Verdict(_))
                            );
                        let score_usable = profile
                            .score
                            .as_ref()
                            .is_some_and(|s| solv::score::effective_score(s, NOW).is_some());
                        if decision.kind == DecisionKind::Approve {
                            approve_count += 1;
                            assert!(score_usable, "approve requires a usable score");
                            assert!(tiers_live, "approve requires a tier verdict");
                        } else if !tiers_live {
                            assert_ne!(
                                decision.kind,
                                DecisionKind::Approve,
                                "G1 violated: system failure became permission to pay"
                            );
                        }
                    }
                }
            }
        }
    }
    assert!(approve_count > 0, "approval must still be reachable");
}

#[test]
fn escalation_rules_match_spec_5_1() {
    let clean_tier1 = tier1_clean();
    // Tier 1 down: escalate.
    assert!(needs_escalation(
        &request(50.0),
        &Tier1Signals::unavailable(),
        Tier::Prime,
        &PolicyConfig::default()
    ));
    // Certain injection: skip Tier 2 entirely.
    let certain = Tier1Signals {
        available: true,
        injection_probability_true: Some(0.95),
        risk_class: Some(RiskClass::None),
        risk_class_confidence: Some(0.97),
    };
    assert!(!needs_escalation(
        &request(50.0),
        &certain,
        Tier::Prime,
        &PolicyConfig::default()
    ));
    // Uncertain band 0.10–0.90.
    let uncertain = Tier1Signals {
        available: true,
        injection_probability_true: Some(0.50),
        risk_class: Some(RiskClass::None),
        risk_class_confidence: Some(0.97),
    };
    assert!(needs_escalation(
        &request(50.0),
        &uncertain,
        Tier::Prime,
        &PolicyConfig::default()
    ));
    // Low risk-class confidence.
    let low_conf = Tier1Signals {
        available: true,
        injection_probability_true: Some(0.02),
        risk_class: Some(RiskClass::None),
        risk_class_confidence: Some(0.70),
    };
    assert!(needs_escalation(
        &request(50.0),
        &low_conf,
        Tier::Prime,
        &PolicyConfig::default()
    ));
    // Cold start.
    assert!(needs_escalation(
        &request(50.0),
        &clean_tier1,
        Tier::Unrated,
        &PolicyConfig::default()
    ));
    // Long description.
    let mut long = request(50.0);
    long.service_description = "x".repeat(1_501);
    assert!(needs_escalation(
        &long,
        &clean_tier1,
        Tier::Prime,
        &PolicyConfig::default()
    ));
    // Borderline amount: above the cap, under twice the cap.
    assert!(needs_escalation(
        &request(600.0),
        &clean_tier1,
        Tier::Prime,
        &PolicyConfig::default()
    ));
    assert!(!needs_escalation(
        &request(1_100.0),
        &clean_tier1,
        Tier::Prime,
        &PolicyConfig::default()
    ));
    // Clean, confident, short, in-cap: no escalation.
    assert!(!needs_escalation(
        &request(50.0),
        &clean_tier1,
        Tier::Prime,
        &PolicyConfig::default()
    ));
}

/// G1 hole closed: a Tier 1 that answered nothing it could parse is not a
/// verdict, even if the HTTP call itself succeeded.
#[test]
fn tier1_available_but_unparsed_fails_closed() {
    let profile = prime_profile();
    let unparsed = Tier1Signals {
        available: true,
        injection_probability_true: None,
        risk_class: None,
        risk_class_confidence: None,
    };
    let decision = run(&request(50.0), Some(&profile), &unparsed, None, &[], 10);
    assert_eq!(decision.kind, DecisionKind::Hold);
    assert_eq!(decision.reasons[0].code, ReasonCode::System003);
}

#[test]
fn non_positive_amounts_are_refused() {
    for amount in [0.0, -50.0, f64::INFINITY] {
        let decision = clean_case(amount);
        assert_eq!(decision.kind, DecisionKind::Block, "amount {amount}");
        assert_eq!(decision.reasons[0].code, ReasonCode::Limit002);
    }
    let decision = clean_case(f64::NAN);
    assert_eq!(decision.kind, DecisionKind::Block);
    // The invalid amount is named as a string, not silently null.
    let evidence = decision.reasons[0].evidence.as_ref().unwrap();
    let output = evidence.detector_output.as_ref().unwrap();
    assert_eq!(output["amount_usd"].as_str(), Some("NaN"));
}

#[test]
fn fired_hold_rule_stays_visible_when_another_row_holds() {
    // Tier cap (row 10) holds first; the learned rule must still appear.
    let profile = profile(Some(settled(67.0, 9, 3))); // STANDARD, cap 100
    let rules = vec![FiredRule {
        rule_id: "015".to_string(),
        effect: RuleEffect::ForcedHold,
    }];
    let decision = run(
        &request(150.0),
        Some(&profile),
        &tier1_clean(),
        None,
        &rules,
        10,
    );
    assert_eq!(decision.kind, DecisionKind::Hold);
    assert_eq!(decision.reasons[0].code, ReasonCode::Limit001);
    assert_eq!(decision.reasons[1].code.as_str(), "SOLV-RULE-015");
    assert!(decision.satisfies_g5());
}

#[test]
fn score010_evidence_names_its_provenance() {
    let decision = clean_case(50.0);
    let evidence = decision.reasons[0].evidence.as_ref().unwrap();
    let output = evidence.detector_output.as_ref().unwrap();
    assert_eq!(output["eval_id"].as_str(), Some("ev_test"));
    assert!(output["settled_at_unix"].as_u64().is_some());
    assert!(output["age_days"].as_u64().is_some());
}

#[test]
fn unparsed_tier1_escalates_so_tier2_can_cover() {
    let unparsed = Tier1Signals {
        available: true,
        injection_probability_true: None,
        risk_class: None,
        risk_class_confidence: None,
    };
    assert!(needs_escalation(
        &request(50.0),
        &unparsed,
        Tier::Prime,
        &PolicyConfig::default()
    ));
}
