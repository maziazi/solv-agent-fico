//! Assembly of the `GET /api/v1/directory` payload (docs/SPEC.md §7.4):
//! every counterparty with its tier, the learned rules (active and shadow),
//! and the metrics block. All read-only views over store snapshots, so the
//! handler stays a one-liner and the shape is testable offline.
//!
//! Honesty rules enforced here: an unrated counterparty says `UNRATED`
//! rather than borrowing a zero; a rule born `shadow` is labelled `shadow`;
//! the Reppo worker is reported as not attached when it is, with
//! `credits_remaining: null` — never an invented quota number.

use serde::Serialize;

use crate::metrics;
use crate::rules::RuleSource;
use crate::score;
use crate::state::{LearnedRule, Profile, Receipt, ReceiptId};

/// Reppo's free quota, for the meter's denominator (docs/DESIGN.md §4.3).
pub const REPPO_QUOTA_TOTAL: u32 = 1_000;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LastDecision {
    pub decision: String,
    pub receipt_id: String,
    pub receipt_url: String,
    pub amount_usd: f64,
    pub created_at_unix: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CounterpartyEntry {
    pub counterparty_id: String,
    pub counterparty_name: String,
    pub target_url: String,
    /// The counterparty's tier today: `UNRATED` until a settled evaluation
    /// backs a score, exactly as the gate sees it.
    pub tier: &'static str,
    pub fico_effective: Option<u16>,
    pub score_age_days: Option<u32>,
    /// The most recent decision recorded for this counterparty, if any.
    pub last_decision: Option<LastDecision>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RuleSourceEntry {
    pub eval_id: String,
    pub criterion: String,
    pub critique_quote: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LearnedRuleEntry {
    pub rule_id: String,
    /// `shadow` until an operator promotes it (SPEC §5.5); the UI must keep
    /// the label visible so a shadow rule never reads as enforcing.
    pub status: String,
    /// Serialises snake_case (`guaranteed_refund`, `forced_hold`), matching
    /// how these values appear in every other payload.
    pub topic: Option<crate::rules::RuleTopic>,
    pub effect: crate::policy::RuleEffect,
    pub threshold: f64,
    pub created_from: Option<RuleSourceEntry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ReppoQuota {
    /// The audit worker is not wired into this server yet: enqueueing is
    /// impossible, so there are no live credits to report. The frontend
    /// shows "worker not attached" instead of a meter pinned at zero.
    pub worker_attached: bool,
    pub credits_remaining: Option<u32>,
    pub quota_total: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DirectoryPayload {
    pub counterparties: Vec<CounterpartyEntry>,
    pub learned_rules: Vec<LearnedRuleEntry>,
    pub reppo: ReppoQuota,
    pub metrics: metrics::Metrics,
}

/// Builds the whole payload from store snapshots. Pure and offline.
pub fn build(
    now_unix: u64,
    profiles: &[(String, Profile)],
    receipts: &[(ReceiptId, Receipt)],
    learned_rules: &[LearnedRule],
) -> DirectoryPayload {
    let counterparties = profiles
        .iter()
        .map(|(id, profile)| counterparty_entry(now_unix, id, profile, receipts))
        .collect();
    let learned_rules = learned_rules.iter().map(rule_entry).collect();
    DirectoryPayload {
        counterparties,
        learned_rules,
        reppo: ReppoQuota {
            worker_attached: false,
            credits_remaining: None,
            quota_total: REPPO_QUOTA_TOTAL,
        },
        metrics: metrics::compute(receipts),
    }
}

/// The latest receipt naming this counterparty in its evidence, or `None`.
/// Receipt ids are time-ordered, so the id order is the recency order.
fn last_decision(id: &str, receipts: &[(ReceiptId, Receipt)]) -> Option<LastDecision> {
    receipts
        .iter()
        .filter(|(_, receipt)| {
            receipt
                .evidence
                .get("request")
                .and_then(|request| request.get("counterparty_id"))
                .and_then(serde_json::Value::as_str)
                == Some(id)
        })
        .max_by(|a, b| a.0.cmp(&b.0))
        .map(|(receipt_id, receipt)| LastDecision {
            decision: receipt.decision.clone(),
            receipt_url: format!("/receipt/{receipt_id}"),
            receipt_id: receipt_id.clone(),
            amount_usd: receipt.amount_usd,
            created_at_unix: receipt.created_at_unix,
        })
}

fn counterparty_entry(
    now_unix: u64,
    id: &str,
    profile: &Profile,
    receipts: &[(ReceiptId, Receipt)],
) -> CounterpartyEntry {
    let effective = profile
        .score
        .as_ref()
        .and_then(|score| score::effective_score(score, now_unix));
    let tier = score::tier_for(effective.map(|effective| effective.fico_effective));
    CounterpartyEntry {
        counterparty_id: id.to_string(),
        counterparty_name: profile.counterparty_name.clone(),
        target_url: profile.target_url.clone(),
        tier: tier.as_str(),
        fico_effective: effective.map(|effective| effective.fico_effective),
        score_age_days: effective.map(|effective| effective.age_days),
        last_decision: last_decision(id, receipts),
    }
}

fn rule_entry(rule: &LearnedRule) -> LearnedRuleEntry {
    LearnedRuleEntry {
        rule_id: rule.rule_id.clone(),
        status: match rule.status {
            crate::state::RuleStatus::Shadow => "shadow",
            crate::state::RuleStatus::Active => "active",
        }
        .to_string(),
        topic: rule.topic,
        effect: rule.effect,
        threshold: rule.threshold,
        created_from: rule.created_from.as_ref().map(source_entry),
    }
}

fn source_entry(source: &RuleSource) -> RuleSourceEntry {
    RuleSourceEntry {
        eval_id: source.eval_id.clone(),
        criterion: source.criterion.clone(),
        critique_quote: source.critique_quote.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::RuleEffect;
    use crate::rules::{RuleSource, RuleTopic};
    use crate::score::SettledScore;
    use crate::state::RuleStatus;
    use serde_json::json;

    fn profile(id: &str, score: Option<SettledScore>) -> (String, Profile) {
        (
            id.to_string(),
            Profile {
                counterparty_name: id.to_string(),
                target_url: format!("https://{id}.example/api"),
                score,
            },
        )
    }

    fn settled(reppo_score: f64, days_ago: u64, now: u64) -> SettledScore {
        SettledScore {
            reppo_score,
            eval_id: "ev_1".to_string(),
            settled_at_unix: now - days_ago * 86_400,
            pillars: crate::score::Pillars {
                reliability: 5,
                track_record: 5,
                safety: 5,
                maturity: 5,
            },
        }
    }

    fn receipt_named(id: &str, counterparty_id: &str, decision: &str) -> (ReceiptId, Receipt) {
        (
            id.to_string(),
            Receipt {
                decision: decision.to_string(),
                amount_usd: 10.0,
                created_at_unix: 1_000,
                evidence_sha256: "ab".to_string(),
                evidence: json!({ "request": { "counterparty_id": counterparty_id } }),
                response: json!({ "tier": "PRIME" }),
            },
        )
    }

    #[test]
    fn unrated_says_unrated_and_borrows_no_number() {
        let now = 10 * 86_400;
        let payload = build(now, &[profile("cp-a", None)], &[], &[]);
        let entry = &payload.counterparties[0];
        assert_eq!(entry.tier, "UNRATED");
        assert_eq!(entry.fico_effective, None);
        assert_eq!(entry.score_age_days, None);
        assert_eq!(entry.last_decision, None);
    }

    #[test]
    fn a_scored_counterparty_carries_tier_age_and_last_decision() {
        let now = 10 * 86_400;
        let profiles = vec![profile("cp-a", Some(settled(84.0, 3, now)))];
        let receipts = vec![
            receipt_named("slv_0002", "cp-a", "APPROVE"),
            receipt_named("slv_0001", "cp-a", "HOLD"),
            receipt_named("slv_0003", "cp-other", "BLOCK"),
        ];
        let entry = &build(now, &profiles, &receipts, &[]).counterparties[0];
        assert_eq!(entry.tier, "PRIME");
        assert_eq!(entry.score_age_days, Some(3));
        let last = entry.last_decision.as_ref().expect("decision recorded");
        assert_eq!(last.decision, "APPROVE");
        assert_eq!(last.receipt_id, "slv_0002", "latest receipt, not the first");
        assert_eq!(last.receipt_url, "/receipt/slv_0002");
    }

    #[test]
    fn shadow_rules_stay_labelled_shadow_with_their_born_from_quote() {
        let rule = LearnedRule {
            rule_id: "SOLV-RULE-001".to_string(),
            status: RuleStatus::Shadow,
            topic: Some(RuleTopic::GuaranteedRefund),
            effect: RuleEffect::ForcedHold,
            threshold: 0.85,
            created_from: Some(RuleSource {
                eval_id: "ev_9".to_string(),
                criterion: "guaranteed refund".to_string(),
                critique_quote: "service promises a refund no process can grant".to_string(),
            }),
        };
        let entry = &build(0, &[], &[], &[rule]).learned_rules[0];
        assert_eq!(entry.status, "shadow");
        let as_json = serde_json::to_value(entry).unwrap();
        assert_eq!(as_json["topic"], "guaranteed_refund");
        assert_eq!(as_json["effect"], "forced_hold");
        let source = entry.created_from.as_ref().expect("source kept");
        assert_eq!(source.eval_id, "ev_9");
        assert_eq!(
            source.critique_quote,
            "service promises a refund no process can grant"
        );
    }

    #[test]
    fn the_quota_reports_no_worker_rather_than_a_zero_meter() {
        let payload = build(0, &[], &[], &[]);
        let reppo = payload.reppo;
        assert!(!reppo.worker_attached);
        assert_eq!(reppo.credits_remaining, None);
        assert_eq!(reppo.quota_total, REPPO_QUOTA_TOTAL);
    }
}
