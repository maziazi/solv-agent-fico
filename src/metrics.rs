//! Bureau metrics computed from stored receipts (docs/SPEC.md §7.4).
//!
//! Every number here is derived from real decisions. A metric without data
//! is `None`/zero and the frontend renders "no data" — nothing in this
//! module may fall back to a plausible constant, because a fabricated
//! measurement is exactly the kind of claim SPEC §9 exists to prevent.

use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::Value;

use crate::state::{Receipt, ReceiptId};

/// Counts per decision kind. The three arms are the whole closed set; a
/// stored decision outside it lands in no bucket rather than being silently
/// reclassified.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct DecisionCounts {
    pub approve: u64,
    pub hold: u64,
    pub block: u64,
}

/// Latency percentiles per path through the gate (SPEC §7.4 "p50/p95 latency
/// per path"). Nearest-rank over the receipts' recorded totals; `None` when a
/// path has no receipts — never zero, which would flatter the service.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct LatencyMsMetrics {
    pub tier1_only_p50: Option<u64>,
    pub tier1_only_p95: Option<u64>,
    pub escalated_p50: Option<u64>,
    pub escalated_p95: Option<u64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Metrics {
    pub receipts_total: u64,
    pub decisions: DecisionCounts,
    /// Distribution of the tier each decision landed on. Receipts without a
    /// readable response (pre-receipt-store) contribute to nothing here.
    pub tiers: BTreeMap<String, u64>,
    pub latency_ms: LatencyMsMetrics,
    /// Sum of `amount_usd` over BLOCK receipts, rounded to cents.
    pub total_blocked_usd: f64,
    /// `None` until receipts record token counts: SPEC §7.4 forbids
    /// computing cost from constants, and no receipt carries them yet.
    pub cost_per_check_usd: Option<f64>,
}

/// Computes metrics over `(id, receipt)` pairs. Pure and offline — the hot
/// path never calls this; the directory endpoint does.
pub fn compute(receipts: &[(ReceiptId, Receipt)]) -> Metrics {
    let mut metrics = Metrics::default();
    let mut tier1_only_totals: Vec<u64> = Vec::new();
    let mut escalated_totals: Vec<u64> = Vec::new();

    for (_, receipt) in receipts {
        metrics.receipts_total += 1;
        match receipt.decision.as_str() {
            "APPROVE" => metrics.decisions.approve += 1,
            "HOLD" => metrics.decisions.hold += 1,
            "BLOCK" => {
                metrics.decisions.block += 1;
                metrics.total_blocked_usd += receipt.amount_usd;
            }
            _ => {}
        }

        if let Some((escalated, total_ms)) = path_latency(receipt) {
            if escalated {
                escalated_totals.push(total_ms);
            } else {
                tier1_only_totals.push(total_ms);
            }
        }
        if let Some(tier) = receipt.response.get("tier").and_then(Value::as_str) {
            *metrics.tiers.entry(tier.to_owned()).or_default() += 1;
        }
    }

    metrics.total_blocked_usd = (metrics.total_blocked_usd * 100.0).round() / 100.0;
    let (p50, p95) = percentiles(&tier1_only_totals);
    metrics.latency_ms.tier1_only_p50 = p50;
    metrics.latency_ms.tier1_only_p95 = p95;
    let (p50, p95) = percentiles(&escalated_totals);
    metrics.latency_ms.escalated_p50 = p50;
    metrics.latency_ms.escalated_p95 = p95;
    metrics
}

/// `(escalated, total_ms)` from a stored response, or `None` when the receipt
/// predates the receipt store, its latency fields are unreadable, or the
/// gate failed closed on an unreachable Tier 1 (`SOLV-SYSTEM-003`). A system
/// failure is not Tier-1 traffic: counting it in the reflex path's sample
/// would put the service's outage profile inside its latency SLO. An
/// unreadable sample is skipped, not read as zero.
fn path_latency(receipt: &Receipt) -> Option<(bool, u64)> {
    let response = receipt.response.as_object()?;
    let tier1_dead = response
        .get("reasons")
        .and_then(Value::as_array)
        .is_some_and(|reasons| {
            reasons
                .iter()
                .any(|reason| reason.get("code").and_then(Value::as_str) == Some("SOLV-SYSTEM-003"))
        });
    if tier1_dead {
        return None;
    }
    let total_ms = response.get("latency_ms")?.get("total")?.as_u64()?;
    let escalated = response
        .get("tier_used")
        .and_then(Value::as_array)
        .is_some_and(|used| {
            used.iter()
                .any(|tier| tier.as_str() == Some("serv-reasoning"))
        });
    Some((escalated, total_ms))
}

/// Nearest-rank percentile pair (p50, p95) over an unsorted sample: the
/// smallest value whose rank is at least `ceil(p/100 · n)`. Empty input is
/// `(None, None)`.
fn percentiles(sample: &[u64]) -> (Option<u64>, Option<u64>) {
    if sample.is_empty() {
        return (None, None);
    }
    let mut sorted = sample.to_vec();
    sorted.sort_unstable();
    let rank = |p: f64| -> u64 {
        let index = (p / 100.0 * sorted.len() as f64).ceil();
        let index = index.clamp(1.0, sorted.len() as f64) as usize;
        sorted[index - 1]
    };
    (Some(rank(50.0)), Some(rank(95.0)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn receipt(decision: &str, amount_usd: f64, response: Value) -> (ReceiptId, Receipt) {
        (
            format!("slv_test_{}_{}", decision, amount_usd),
            Receipt {
                decision: decision.to_string(),
                amount_usd,
                created_at_unix: 0,
                evidence_sha256: String::new(),
                evidence: json!({}),
                response,
            },
        )
    }

    fn response(tier: &str, escalated: bool, total_ms: u64) -> Value {
        let tier_used = if escalated {
            json!(["clef-flash", "serv-reasoning"])
        } else {
            json!(["clef-flash"])
        };
        json!({
            "tier": tier,
            "tier_used": tier_used,
            "latency_ms": { "state": 0, "tier1": total_ms - 1, "total": total_ms },
        })
    }

    #[test]
    fn empty_receipts_yield_none_not_zero() {
        let metrics = compute(&[]);
        assert_eq!(metrics.receipts_total, 0);
        assert_eq!(metrics.latency_ms.tier1_only_p50, None);
        assert_eq!(metrics.latency_ms.escalated_p95, None);
        assert_eq!(metrics.total_blocked_usd, 0.0);
        assert_eq!(metrics.cost_per_check_usd, None, "no tokens, no cost claim");
    }

    #[test]
    fn counts_decisions_tiers_and_blocked_total() {
        let receipts = vec![
            receipt("APPROVE", 18.0, response("PRIME", false, 44)),
            receipt("HOLD", 12.5, response("UNRATED", false, 61)),
            receipt("BLOCK", 50.0, response("HIGH_RISK", true, 671)),
            receipt("BLOCK", 0.3, response("HIGH_RISK", false, 55)),
        ];
        let metrics = compute(&receipts);
        assert_eq!(metrics.receipts_total, 4);
        assert_eq!(metrics.decisions.approve, 1);
        assert_eq!(metrics.decisions.hold, 1);
        assert_eq!(metrics.decisions.block, 2);
        assert_eq!(metrics.total_blocked_usd, 50.3);
        assert_eq!(metrics.tiers["PRIME"], 1);
        assert_eq!(metrics.tiers["UNRATED"], 1);
        assert_eq!(metrics.tiers["HIGH_RISK"], 2);
    }

    #[test]
    fn paths_split_by_tier_used_and_use_nearest_rank_percentiles() {
        let receipts = vec![
            receipt("APPROVE", 1.0, response("PRIME", false, 40)),
            receipt("APPROVE", 1.0, response("PRIME", false, 50)),
            receipt("HOLD", 1.0, response("UNRATED", false, 60)),
            receipt("BLOCK", 1.0, response("HIGH_RISK", true, 600)),
            receipt("BLOCK", 1.0, response("HIGH_RISK", true, 700)),
            receipt("BLOCK", 1.0, response("HIGH_RISK", true, 800)),
        ];
        let metrics = compute(&receipts);
        // Tier-1-only path: sorted [40, 50, 60]; p50 rank ceil(1.5) = 2 -> 50;
        // p95 rank ceil(2.85) = 3 -> 60.
        assert_eq!(metrics.latency_ms.tier1_only_p50, Some(50));
        assert_eq!(metrics.latency_ms.tier1_only_p95, Some(60));
        // Escalated path: sorted [600, 700, 800].
        assert_eq!(metrics.latency_ms.escalated_p50, Some(700));
        assert_eq!(metrics.latency_ms.escalated_p95, Some(800));
    }

    #[test]
    fn receipts_without_a_readable_response_count_but_sample_nothing() {
        let receipts = vec![
            receipt("HOLD", 9.0, Value::Null),
            receipt("APPROVE", 9.0, json!({ "tier": "PRIME" })),
        ];
        let metrics = compute(&receipts);
        assert_eq!(metrics.receipts_total, 2);
        assert_eq!(metrics.decisions.hold, 1);
        assert_eq!(
            metrics.latency_ms.tier1_only_p50, None,
            "no latency, no sample"
        );
        assert_eq!(
            metrics.tiers.get("PRIME"),
            Some(&1),
            "tier counted when present"
        );
    }

    #[test]
    fn a_fail_closed_tier1_failure_is_not_tier1_traffic() {
        // The reviewer's live probe: an unconfigured provider yields
        // tier_used: ["clef-flash"] with a near-zero latency and SOLV-SYSTEM-003.
        // That receipt must sample no path, or the reflex SLO absorbs outages.
        let fail_closed = json!({
            "tier": "UNRATED",
            "tier_used": ["clef-flash"],
            "reasons": [{ "code": "SOLV-SYSTEM-003", "severity": "critical" }],
            "latency_ms": { "state": 0, "tier1": 0, "total": 1 }
        });
        let receipts = vec![
            receipt("HOLD", 1.0, fail_closed),
            receipt("APPROVE", 1.0, response("PRIME", false, 50)),
        ];
        let metrics = compute(&receipts);
        assert_eq!(metrics.receipts_total, 2);
        assert_eq!(
            metrics.decisions.hold, 1,
            "the failure still counts as a decision"
        );
        assert_eq!(
            metrics.latency_ms.tier1_only_p50,
            Some(50),
            "only real reflex samples"
        );
        assert_eq!(metrics.latency_ms.tier1_only_p95, Some(50));
        assert_eq!(metrics.latency_ms.escalated_p50, None);
    }

    #[test]
    fn an_unrecognised_decision_is_counted_but_classified_nowhere() {
        let receipts = vec![receipt("MAYBE", 9.0, response("PRIME", false, 40))];
        let metrics = compute(&receipts);
        assert_eq!(metrics.receipts_total, 1);
        assert_eq!(
            metrics.decisions,
            DecisionCounts::default(),
            "an unknown decision lands in no bucket"
        );
        assert_eq!(metrics.total_blocked_usd, 0.0);
    }
}
