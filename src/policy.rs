//! The deterministic policy gate (docs/SPEC.md §5.4).
//!
//! One pure function: no I/O, no model calls, no clock reads — everything
//! time-dependent arrives as a parameter. Models supply signals; this gate
//! decides. The row order below is binding: hard gates first, so a good
//! track record can never rescue a hijack attempt.

use crate::reason::{Effect, Evidence, Reason, ReasonCode, Source};

/// Probability of injection at which Tier 1 counts as certain (SPEC §5.1,
/// §5.4 row 2): Tier 2 is skipped and the decision is BLOCK.
const INJECTION_CERTAINTY: f64 = 0.90;
/// Below this injection probability the band is considered clean; between
/// the two bounds it is uncertain and escalates (SPEC §5.1 rule 1).
const INJECTION_UNCERTAIN_FLOOR: f64 = 0.10;
/// Risk-class confidence below which the classification escalates
/// (SPEC §5.1 rule 2, and row 3's blocking confidence floor).
const RISK_CONFIDENCE_FLOOR: f64 = 0.90;
/// Descriptions longer than this escalate to Tier 2 (SPEC §5.1 rule 4).
const DESCRIPTION_ESCALATION_CHARS: usize = 1_500;
use crate::score::{self, Caps, SettledScore, Tier};
use crate::state::Profile;
use serde::{Deserialize, Serialize};

/// Operator-configurable policy parameters (SPEC §4.3, §5.4 `cfg`).
/// Whichever values were active when a decision was made are folded into
/// `ruleset_version` and the evidence hash (with PR-08).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PolicyConfig {
    pub caps: Caps,
    /// Below this effective score the decision is BLOCK (`SOLV-SCORE-001`).
    pub block_floor: u16,
    /// A safety pillar at or below this forces BLOCK (`SOLV-SCORE-002`).
    pub safety_floor: u8,
    /// No amount above this is ever approved (`SOLV-LIMIT-002`).
    pub absolute_cap_usd: f64,
    /// Hard end-to-end ceiling for an escalated decision (SPEC §2.1, G2).
    pub latency_ceiling_ms: u64,
}

impl Default for PolicyConfig {
    fn default() -> Self {
        Self {
            caps: Caps::default(),
            block_floor: 550,
            safety_floor: 3,
            absolute_cap_usd: 500.00,
            latency_ceiling_ms: 2_000,
        }
    }
}

/// A pre-spend request as it enters the gate (SPEC §7.1).
#[derive(Debug, Clone, PartialEq)]
pub struct PreSpendRequest {
    pub counterparty_name: String,
    pub target_url: String,
    pub amount_usd: f64,
    pub service_description: String,
}

/// Tier 1 (Clef-flash) signals. Produced by the Tier 1 client (PR-05);
/// `available: false` means the call failed or timed out.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tier1Signals {
    pub available: bool,
    pub injection_probability_true: Option<f64>,
    pub risk_class: Option<RiskClass>,
    pub risk_class_confidence: Option<f64>,
}

impl Tier1Signals {
    /// A Tier 1 that never ran.
    pub fn unavailable() -> Self {
        Self {
            available: false,
            injection_probability_true: None,
            risk_class: None,
            risk_class_confidence: None,
        }
    }

    /// True only when the call succeeded *and* at least one answer was
    /// parsed. An HTTP 200 with an unreadable body is not a verdict, and
    /// G1 forbids reaching APPROVE on it — system failure must never
    /// become permission to pay.
    pub fn has_verdict(&self) -> bool {
        self.available && (self.injection_probability_true.is_some() || self.risk_class.is_some())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RiskClass {
    None,
    Overclaim,
    Coercion,
    Exfiltration,
    Mismatch,
}

/// Tier 2 (SERV Reasoning) outcome (SPEC §5.2). A guard refusal is positive
/// evidence of injection, not a failed call.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Tier2Outcome {
    GuardRefusal,
    Verdict(Tier2Verdict),
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier2Verdict {
    Approve,
    Revise,
    Mismatch,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tier2Signals {
    pub outcome: Tier2Outcome,
}

/// A learned rule that fired on this request (SPEC §5.5). Firing is
/// evaluated where rules compile into Tier 1 questions (PR-10); the gate
/// only consumes the result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FiredRule {
    pub rule_id: String,
    pub effect: RuleEffect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleEffect {
    ForcedBlock,
    ForcedHold,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum DecisionKind {
    Approve,
    Hold,
    Block,
}

impl DecisionKind {
    pub fn as_str(self) -> &'static str {
        match self {
            DecisionKind::Approve => "APPROVE",
            DecisionKind::Hold => "HOLD",
            DecisionKind::Block => "BLOCK",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Decision {
    pub kind: DecisionKind,
    pub reasons: Vec<Reason>,
    pub tier: Tier,
    pub max_safe_spend_usd: f64,
    /// The effective score the decision rested on, if any.
    pub effective_score: Option<score::EffectiveScore>,
}

impl Decision {
    /// G5 (SPEC §3.3): `reasons` is never empty and every reason satisfies
    /// the five conditions of §2.2.
    pub fn satisfies_g5(&self) -> bool {
        !self.reasons.is_empty() && self.reasons.iter().all(|reason| reason.satisfies_g5())
    }
}

fn evidence(source: Source, field: &str, detector_output: serde_json::Value) -> Evidence {
    Evidence {
        source,
        field: field.to_string(),
        quote: None,
        locator: None,
        detector_output: Some(detector_output),
    }
}

/// Evidence for a reason that accuses the counterparty's text (SPEC §2.2.3):
/// it carries the verbatim span and its char locator when the span finder
/// can point at the attack. A detector verdict without a located span stays
/// quote-free — an invented quote would be worse than none.
fn attack_evidence(
    source: Source,
    field: &str,
    detector_output: serde_json::Value,
    description: &str,
) -> Evidence {
    let mut ev = evidence(source, field, detector_output);
    if let Some(span) = crate::spans::injection_span(description) {
        // §6.1: a quote is shown at most 240 characters, on a character
        // boundary — and the locator keeps pointing at exactly the quoted
        // subrange, so honesty survives the cut.
        let span = crate::spans::clamp_to_chars(span, description, crate::reason::MAX_QUOTE_CHARS);
        ev.locator = Some(span.locator());
        ev.quote = Some(span.quote);
    }
    ev
}

/// Evaluates the SPEC §5.4 decision table in binding row order.
///
/// Deviations from the written signature, both documented in the PR:
/// `now_unix` and `elapsed_ms` arrive as parameters to keep the function
/// pure; row 10 is split into the operator's absolute cap (`SOLV-LIMIT-002`,
/// taxonomy §6.2, `forced_block`) ahead of the tier cap (`SOLV-LIMIT-001`),
/// and a fired `forced_hold` rule holds ahead of approval.
#[allow(clippy::too_many_arguments)]
pub fn decide(
    request: &PreSpendRequest,
    profile: Option<&Profile>,
    now_unix: u64,
    elapsed_ms: u64,
    tier1: &Tier1Signals,
    tier2: Option<&Tier2Signals>,
    fired_rules: &[FiredRule],
    config: &PolicyConfig,
) -> Decision {
    let settled: Option<&SettledScore> = profile.and_then(|p| p.score.as_ref());
    let effective = settled.and_then(|s| score::effective_score(s, now_unix));
    let tier = score::tier_for(effective.map(|s| s.fico_effective));
    let cap = tier.max_safe_spend_usd(&config.caps);

    // Reasons for learned rules that fired with a forcing-hold effect.
    // They attach to every HOLD outcome — including rows 7–11 — so an
    // auditor always sees that a guardrail fired, whichever row held first.
    let forced_hold_rule_reasons: Vec<Reason> = fired_rules
        .iter()
        .filter(|rule| rule.effect == RuleEffect::ForcedHold)
        .map(|rule| {
            let mut reason = Reason::from_code(ReasonCode::Rule(rule.rule_id.clone()));
            reason.effect = Effect::ForcedHold;
            reason.evidence = Some(evidence(
                Source::PolicyGate,
                "learned_rules",
                serde_json::json!({ "rule_id": rule.rule_id }),
            ));
            reason
        })
        .collect();

    let hold = |code: ReasonCode, ev: Evidence| {
        let mut reasons = Vec::with_capacity(1 + forced_hold_rule_reasons.len());
        let mut primary = Reason::from_code(code);
        primary.evidence = Some(ev);
        reasons.push(primary);
        reasons.extend(forced_hold_rule_reasons.iter().cloned());
        finish(DecisionKind::Hold, reasons, effective, tier, cap)
    };
    let block = |code: ReasonCode, ev: Evidence| blocking(code, Some(ev), effective, tier, cap);

    // Row 1 — Tier 2 guard refusal is positive evidence of injection.
    if let Some(t2) = tier2 {
        if t2.outcome == Tier2Outcome::GuardRefusal {
            return block(
                ReasonCode::Injection001,
                attack_evidence(
                    Source::ServPromptGuard,
                    "service_description",
                    serde_json::json!({ "refusal": true }),
                    &request.service_description,
                ),
            );
        }
    }

    // Row 2 — Tier 1 is confident an instruction targets the agent.
    if tier1.available
        && tier1
            .injection_probability_true
            .is_some_and(|p| p >= INJECTION_CERTAINTY)
    {
        return block(
            ReasonCode::Injection002,
            attack_evidence(
                Source::ClefFlash,
                "service_description",
                serde_json::json!({ "probability_true": tier1.injection_probability_true }),
                &request.service_description,
            ),
        );
    }

    // Row 3 — risk class exfiltration or coercion at high confidence.
    if tier1.available {
        let risky = matches!(
            tier1.risk_class,
            Some(RiskClass::Exfiltration) | Some(RiskClass::Coercion)
        );
        let confident = tier1
            .risk_class_confidence
            .is_some_and(|c| c >= RISK_CONFIDENCE_FLOOR);
        if risky && confident {
            return block(
                ReasonCode::Injection003,
                attack_evidence(
                    Source::ClefFlash,
                    "service_description",
                    serde_json::json!({
                        "risk_class": tier1.risk_class,
                        "confidence": tier1.risk_class_confidence,
                    }),
                    &request.service_description,
                ),
            );
        }
    }

    // Row 4 — an active learned rule fired with a forcing-block effect.
    if let Some(rule) = fired_rules
        .iter()
        .find(|rule| rule.effect == RuleEffect::ForcedBlock)
    {
        let mut reason = Reason::from_code(ReasonCode::Rule(rule.rule_id.clone()));
        reason.effect = Effect::ForcedBlock;
        reason.evidence = Some(evidence(
            Source::PolicyGate,
            "learned_rules",
            serde_json::json!({ "rule_id": rule.rule_id }),
        ));
        return finish(DecisionKind::Block, vec![reason], effective, tier, cap);
    }

    // Row 5 — safety pillar at or below the floor. Fires on the settled
    // record even when the score has expired: stale evidence of a hijack
    // attempt still fails closed, and the evidence says it is stale.
    if let Some(score) = settled {
        if score.pillars.safety <= config.safety_floor {
            return block(
                ReasonCode::Score002,
                evidence(
                    Source::Reppo,
                    "pillars.safety",
                    serde_json::json!({
                        "safety": score.pillars.safety,
                        "expired": effective.is_none(),
                    }),
                ),
            );
        }
    }

    // Row 6 — effective score below the block floor.
    if let Some(effective) = effective {
        if effective.fico_effective < config.block_floor {
            return block(
                ReasonCode::Score001,
                evidence(
                    Source::Reppo,
                    "fico_effective",
                    serde_json::json!({
                        "fico_effective": effective.fico_effective,
                        "block_floor": config.block_floor,
                    }),
                ),
            );
        }
    }

    // Row 7 — no tier produced a verdict (both down, or Tier 1 answered
    // nothing it could parse): fail closed. A Tier 2 entry present as
    // Unavailable means the orchestrator escalated and the call failed —
    // the escalation's verdict is missing, so the request holds too
    // (G1: system failure never becomes permission to pay).
    let tier2_covering = tier2.is_some_and(|t2| {
        matches!(
            t2.outcome,
            Tier2Outcome::GuardRefusal | Tier2Outcome::Verdict(_)
        )
    });
    if !tier1.has_verdict() && !tier2_covering {
        return hold(
            ReasonCode::System003,
            evidence(
                Source::PolicyGate,
                "tier_availability",
                serde_json::json!({}),
            ),
        );
    }

    // Row 8 — the latency ceiling passed: HOLD, never APPROVE (G2).
    if elapsed_ms > config.latency_ceiling_ms {
        return hold(
            ReasonCode::System002,
            evidence(
                Source::PolicyGate,
                "latency",
                serde_json::json!({ "elapsed_ms": elapsed_ms, "ceiling_ms": config.latency_ceiling_ms }),
            ),
        );
    }

    // Row 9 — cold start: no settled evaluation means no number.
    if tier == Tier::Unrated {
        return hold(
            ReasonCode::Coldstart001,
            evidence(
                Source::PolicyGate,
                "profile.score",
                serde_json::json!({ "settled_score": settled.is_some(), "expired": settled.is_some() && effective.is_none() }),
            ),
        );
    }

    // Row 10 — amount against the operator's absolute cap, then the tier
    // cap. A non-finite or non-positive amount is rejected here too: it
    // could never be a payment, and minting an APPROVE receipt for it
    // would pollute the audit record (G4). NaN serialises as null, so the
    // invalid amount is also named as a string.
    let invalid_amount = !request.amount_usd.is_finite() || request.amount_usd <= 0.0;
    if invalid_amount || request.amount_usd > config.absolute_cap_usd {
        let amount_json = if request.amount_usd.is_finite() {
            serde_json::json!(request.amount_usd)
        } else {
            serde_json::json!(format!("{}", request.amount_usd))
        };
        return block(
            ReasonCode::Limit002,
            evidence(
                Source::PolicyGate,
                "amount_usd",
                serde_json::json!({
                    "amount_usd": amount_json,
                    "invalid": invalid_amount,
                    "absolute_cap_usd": config.absolute_cap_usd,
                }),
            ),
        );
    }
    if request.amount_usd > cap {
        return hold(
            ReasonCode::Limit001,
            evidence(
                Source::PolicyGate,
                "amount_usd",
                serde_json::json!({
                    "amount_usd": request.amount_usd,
                    "max_safe_spend_usd": cap,
                    "tier": tier.as_str(),
                }),
            ),
        );
    }

    // Row 11 — Tier 2 asked for revision, or found a claim-evidence mismatch.
    if let Some(t2) = tier2 {
        if let Tier2Outcome::Verdict(verdict) = t2.outcome {
            match verdict {
                Tier2Verdict::Revise => {
                    return hold(
                        ReasonCode::Reason001,
                        evidence(
                            Source::ServReasoning,
                            "service_description",
                            serde_json::json!({ "verdict": "revise" }),
                        ),
                    );
                }
                Tier2Verdict::Mismatch => {
                    return hold(
                        ReasonCode::Reason002,
                        evidence(
                            Source::ServReasoning,
                            "service_description",
                            serde_json::json!({ "verdict": "mismatch" }),
                        ),
                    );
                }
                Tier2Verdict::Approve => {}
            }
        }
    }

    // A required escalation that failed can never approve: a Tier 2 entry
    // present as Unavailable means the orchestrator escalated and the call
    // failed (G1 — system failure never becomes permission to pay). It is
    // evaluated after every SPEC row so a more specific hold (cold start,
    // cap, revision) still names itself on the receipt.
    if matches!(
        tier2,
        Some(Tier2Signals {
            outcome: Tier2Outcome::Unavailable
        })
    ) {
        return hold(
            ReasonCode::System003,
            evidence(
                Source::PolicyGate,
                "tier_availability",
                serde_json::json!({ "escalation_failed": true }),
            ),
        );
    }

    // A learned rule fired with a forcing-hold effect and nothing harder
    // fired first: the rule alone decides.
    if !forced_hold_rule_reasons.is_empty() {
        return finish(
            DecisionKind::Hold,
            forced_hold_rule_reasons.clone(),
            effective,
            tier,
            cap,
        );
    }

    // Row 12 — nothing fired: approve, with reasons (G5).
    let mut reason = Reason::from_code(ReasonCode::Score010);
    if let (Some(score), Some(effective)) = (settled, effective) {
        reason.message = format!(
            "Score and pillars meet the approval conditions: reliability {}/10, track record {}/10, safety {}/10, maturity {}/10.",
            score.pillars.reliability,
            score.pillars.track_record,
            score.pillars.safety,
            score.pillars.maturity
        );
        reason.evidence = Some(evidence(
            Source::Reppo,
            "pillars",
            serde_json::json!({
                "fico_effective": effective.fico_effective,
                "age_days": effective.age_days,
                "eval_id": score.eval_id,
                "settled_at_unix": score.settled_at_unix,
            }),
        ));
    }
    finish(DecisionKind::Approve, vec![reason], effective, tier, cap)
}

fn blocking(
    code: ReasonCode,
    ev: Option<Evidence>,
    effective: Option<score::EffectiveScore>,
    tier: Tier,
    cap: f64,
) -> Decision {
    let mut reason = Reason::from_code(code);
    reason.evidence = ev;
    finish(DecisionKind::Block, vec![reason], effective, tier, cap)
}

fn finish(
    kind: DecisionKind,
    mut reasons: Vec<Reason>,
    effective: Option<score::EffectiveScore>,
    tier: Tier,
    cap: f64,
) -> Decision {
    // Companion reason (SPEC §6.2 SOLV-SCORE-003): whenever decay reduced
    // the score, say so — a lowered cap must be visible on the receipt.
    if let Some(effective) = effective {
        if effective.decay > 0 {
            let mut reason = Reason::from_code(ReasonCode::Score003);
            reason.message = format!(
                "The score was reduced by {} points because the underlying evidence is {} days old.",
                effective.decay, effective.age_days
            );
            reason.evidence = Some(evidence(
                Source::Reppo,
                "score_age",
                serde_json::json!({
                    "decay": effective.decay,
                    "age_days": effective.age_days,
                    "fico_raw": effective.fico_raw,
                    "fico_effective": effective.fico_effective,
                }),
            ));
            reasons.push(reason);
        }
    }
    Decision {
        kind,
        reasons,
        tier,
        max_safe_spend_usd: cap,
        effective_score: effective,
    }
}

/// The escalation rules of SPEC §5.1, applied deterministically. Tier 2 is
/// consulted only when at least one rule fires — and never when Tier 1 is
/// already certain of injection (≥ 0.90), where discussion only adds latency
/// and cost. A Tier 1 failure escalates (`SOLV-SYSTEM-001`, taxonomy §6.2).
pub fn needs_escalation(
    request: &PreSpendRequest,
    tier1: &Tier1Signals,
    tier: Tier,
    config: &PolicyConfig,
) -> bool {
    let cap_usd = tier.max_safe_spend_usd(&config.caps);
    // Escalate on a Tier 1 that produced no verdict — down or unparsed —
    // so Tier 2 gets the chance to cover (SOLV-SYSTEM-001, taxonomy §6.2).
    if !tier1.has_verdict() {
        return true;
    }
    if tier1
        .injection_probability_true
        .is_some_and(|p| p >= INJECTION_CERTAINTY)
    {
        return false;
    }
    if tier1
        .injection_probability_true
        .is_some_and(|p| p > INJECTION_UNCERTAIN_FLOOR && p < INJECTION_CERTAINTY)
    {
        return true;
    }
    if tier1
        .risk_class_confidence
        .is_some_and(|c| c < RISK_CONFIDENCE_FLOOR)
    {
        return true;
    }
    if tier == Tier::Unrated {
        return true;
    }
    if request.service_description.chars().count() > DESCRIPTION_ESCALATION_CHARS {
        return true;
    }
    // Borderline: above the cap but under twice the cap.
    request.amount_usd > cap_usd && request.amount_usd < 2.0 * cap_usd
}
