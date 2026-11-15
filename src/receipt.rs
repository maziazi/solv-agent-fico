//! Receipts and the evidence hash (docs/SPEC.md §3.3 G4, §6.3, §7.3).
//!
//! Every `pre-spend-check` decision is sealed into a receipt: the evidence
//! document of §6.3, its SHA-256, and the response that was returned. The
//! evidence is stored whole so any third party can recompute the hash.
//!
//! Canonical form, which a verifier must reproduce byte for byte: object
//! keys sorted by their UTF-8 bytes, no whitespace, strings escaped per
//! RFC 8259 with only the mandatory escapes (non-ASCII stays raw), numbers
//! in shortest round-trip form with integral floats keeping their `.0`
//! (what Rust's `serde_json` and Python's `json.dumps` both print). The
//! hash is the lowercase hex SHA-256 of that text.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

use crate::AppState;
use crate::api::iso8601_utc;
use crate::policy::{Decision, PolicyConfig};
use crate::score::{EffectiveScore, SettledScore};

/// Version of the rule set a decision was made under. Bumped whenever the
/// gate's rows or the default policy values change; learned rules
/// (PR-10) will make it dynamic. It is part of the hashed evidence.
pub const RULESET_VERSION: u32 = 1;

/// Lowercase hex SHA-256.
pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Serialises `value` in the canonical form described in the module docs.
/// Sorting is done here rather than trusted to `serde_json`'s map type,
/// which a transitive feature flag can switch to insertion order.
pub fn canonical_json(value: &Value) -> String {
    let mut out = String::new();
    write_canonical(value, &mut out);
    out
}

fn write_canonical(value: &Value, out: &mut String) {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            out.push('{');
            for (index, key) in keys.into_iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                out.push_str(&Value::String(key.clone()).to_string());
                out.push(':');
                write_canonical(&map[key], out);
            }
            out.push('}');
        }
        Value::Array(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                write_canonical(item, out);
            }
            out.push(']');
        }
        scalar => out.push_str(&scalar.to_string()),
    }
}

/// `evidence_sha256` (SPEC §6.3): SHA-256 of the canonical evidence JSON.
pub fn evidence_sha256(evidence: &Value) -> String {
    sha256_hex(canonical_json(evidence).as_bytes())
}

/// What Tier 1 contributed. `answers` is `None` when the call failed or
/// timed out — the receipt then says so instead of inventing signals.
pub struct Tier1Evidence<'a> {
    pub model: &'a str,
    pub answers: Option<&'a Map<String, Value>>,
    pub latency_ms: u64,
}

/// What Tier 2 contributed; absent from the evidence when it did not run.
pub struct Tier2Evidence<'a> {
    pub model: &'a str,
    pub reasoning_effort: &'a str,
    pub verdict: &'a str,
    pub guard_triggered: bool,
    pub latency_ms: u64,
}

pub struct EvidenceInput<'a> {
    pub counterparty_id: &'a str,
    pub target_url: &'a str,
    pub amount_usd: f64,
    pub service_description: &'a str,
    pub settled: Option<&'a SettledScore>,
    pub effective: Option<EffectiveScore>,
    pub tier1: Tier1Evidence<'a>,
    pub tier2: Option<Tier2Evidence<'a>>,
    pub rules_applied: &'a [String],
    pub config: &'a PolicyConfig,
    pub decision: &'a Decision,
}

/// Builds the evidence document of SPEC §6.3.
///
/// The description enters only as its SHA-256, so the hash stays stable
/// and a public receipt is not a billboard for an attacker's payload; the
/// bounded verbatim text lives in `reasons[].evidence.quote` (§6.1).
/// `profile_snapshot` is `null` for an UNRATED counterparty rather than an
/// object of empty fields masquerading as an audit.
pub fn build_evidence(input: &EvidenceInput<'_>) -> Value {
    let profile_snapshot = input.settled.map(|settled| {
        json!({
            "fico_raw": input.effective.map(|e| e.fico_raw),
            "fico_effective": input.effective.map(|e| e.fico_effective),
            "decay": input.effective.map(|e| e.decay),
            "age_days": input.effective.map(|e| e.age_days),
            "pillars": settled.pillars,
            "eval_id": settled.eval_id,
            "settled_at": iso8601_utc(settled.settled_at_unix),
        })
    });

    let tier1 = json!({
        "model": input.tier1.model,
        "answers": input.tier1.answers,
        "confidences": input.tier1.answers.map(confidences),
        "latency_ms": input.tier1.latency_ms,
    });

    let tier2 = input.tier2.as_ref().map(|tier2| {
        json!({
            "model": tier2.model,
            "reasoning_effort": tier2.reasoning_effort,
            "verdict": tier2.verdict,
            "guard_triggered": tier2.guard_triggered,
            "latency_ms": tier2.latency_ms,
        })
    });

    json!({
        "request": {
            "counterparty_id": input.counterparty_id,
            "target_url": input.target_url,
            "amount_usd": input.amount_usd,
            "description_sha256": sha256_hex(input.service_description.as_bytes()),
        },
        "profile_snapshot": profile_snapshot,
        "tier1": tier1,
        "tier2": tier2,
        "rules_applied": input.rules_applied,
        "ruleset_version": RULESET_VERSION,
        "policy_config": input.config,
        "decision": {
            "decision": input.decision.kind.as_str(),
            "reasons": input.decision.reasons,
            "max_safe_spend": input.decision.max_safe_spend_usd,
        },
    })
}

/// The numeric confidence of every Tier 1 answer: `probability_true` for
/// `noul` questions, `confidence` for choice questions.
fn confidences(answers: &Map<String, Value>) -> Value {
    let mut out = Map::new();
    for (question, answer) in answers {
        let number = answer
            .get("probability_true")
            .or_else(|| answer.get("confidence"))
            .filter(|value| value.is_number());
        if let Some(number) = number {
            out.insert(question.clone(), number.clone());
        }
    }
    Value::Object(out)
}

/// `GET /api/v1/receipt/:id` and `GET /receipt/:id` (SPEC §7.3): the
/// receipt as JSON — the original response plus the full evidence
/// document and its creation time. Read-only and unauthenticated. The
/// bare `/receipt/:id` route is the one PR-12 turns into the HTML page.
pub async fn get_receipt(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    let Some(receipt) = state.store().receipt(&id) else {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "receipt not found" })),
        )
            .into_response();
    };
    let mut body = receipt.response;
    if let Value::Object(map) = &mut body {
        map.insert(
            "created_at".to_string(),
            Value::String(iso8601_utc(receipt.created_at_unix)),
        );
        map.insert("evidence".to_string(), receipt.evidence);
    }
    // Receipts carry untrusted counterparty quotes: forbid content sniffing.
    ([(header::X_CONTENT_TYPE_OPTIONS, "nosniff")], Json(body)).into_response()
}
