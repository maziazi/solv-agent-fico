//! Receipts and the evidence hash (docs/SPEC.md §3.3 G4, §6.3, §7.3).
//!
//! Every `pre-spend-check` decision is sealed into a receipt: the evidence
//! document of §6.3, its SHA-256, and the response that was returned. The
//! evidence is stored whole so any third party can recompute the hash.
//!
//! Canonical form, which a verifier must reproduce byte for byte, is the
//! output of Python's `json.dumps(v, sort_keys=True, separators=(",", ":"),
//! ensure_ascii=False)`: object keys sorted by code point (the same order
//! as their UTF-8 bytes), no whitespace, strings escaped per RFC 8259 with
//! only the mandatory escapes (non-ASCII stays raw), and floats in
//! Python's `repr` form. That float form differs from `serde_json`'s below
//! 1e-4 and above 1e16, so it is implemented here ([`format_float`]) and
//! never delegated. This is not RFC 8785 (JCS). The reference verifier is
//! `tools/recompute_receipt.py`; the hash is the lowercase hex SHA-256 of
//! the canonical text.

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

/// Base version of the rule set a decision was made under. Bumped whenever
/// the gate's rows or the default policy values change. A decision's
/// `ruleset_version` is this plus the number of changes to the active
/// learned-rule set (`Store::compiled_rules`), so it is `1` until an
/// operator promotes a rule. It is part of the hashed evidence.
/// v2 (2026-11-15): five FICO-aligned bands replace the four ad-hoc tiers,
/// per-band caps replace the three tier caps, and the block floor moves
/// 550 → 580 (`docs/TIERS-V2.md`). Band names and `Caps` field names are
/// part of the hashed evidence, so a v1 receipt and a v2 receipt are not
/// comparable and must not be read as if they were.
pub const RULESET_VERSION: u32 = 2;

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
        Value::Number(number) => match number.as_f64().filter(|_| number.is_f64()) {
            Some(float) => out.push_str(&format_float(float)),
            None => out.push_str(&number.to_string()),
        },
        scalar => out.push_str(&scalar.to_string()),
    }
}

/// The shortest round-trip digits of `value` in `d.ddde±x` form, choosing
/// among equally short candidates the way Python does.
///
/// Rust's shortest printer and Python's `repr` agree on the length, but when
/// the exact value sits precisely halfway between two candidates of that
/// length (`97280982573650.625`) Rust picks the upper one and Python the even
/// one (`...62`). So: take the length from Rust, re-print the exact value at
/// that length (Rust's fixed-precision printer rounds half to even), and keep
/// it when it still round-trips. If it does not (an asymmetric rounding
/// interval at a power of two), Rust's own shortest digits are the answer.
fn python_digits(value: f64) -> String {
    let shortest = format!("{value:e}");
    let digit_count = shortest
        .split_once('e')
        .map_or(0, |(mantissa, _)| {
            mantissa.chars().filter(char::is_ascii_digit).count()
        })
        .max(1);
    let rounded = format!("{:.*e}", digit_count - 1, value);
    if rounded.parse::<f64>() == Ok(value) {
        rounded
    } else {
        shortest
    }
}

/// Formats a finite float exactly as Python's `repr(float)` does, which is
/// what the reference verifier prints.
///
/// Digits are the shortest that round-trip ([`python_digits`]). With `exp10`
/// the decimal exponent of the leading digit: decimal notation when
/// `-4 <= exp10 < 16`, with integral values keeping a trailing `.0`;
/// otherwise scientific with a signed exponent of at least two digits
/// (`1e-05`, `1e+16`, `1.5e+300`, `5e-324`). Zero is `0.0` / `-0.0`.
pub fn format_float(value: f64) -> String {
    debug_assert!(value.is_finite(), "JSON has no NaN or infinity");
    if value == 0.0 {
        return if value.is_sign_negative() {
            "-0.0"
        } else {
            "0.0"
        }
        .to_string();
    }
    let scientific = python_digits(value);
    let (mantissa, exponent) = scientific
        .split_once('e')
        .expect("LowerExp always writes an exponent");
    let exp10: i32 = exponent.parse().expect("the exponent is an integer");
    let (sign, mantissa) = match mantissa.strip_prefix('-') {
        Some(unsigned) => ("-", unsigned),
        None => ("", mantissa),
    };
    let digits: String = mantissa.chars().filter(|c| *c != '.').collect();

    if !(-4..16).contains(&exp10) {
        let (first, rest) = digits.split_at(1);
        let mantissa = if rest.is_empty() {
            first.to_string()
        } else {
            format!("{first}.{rest}")
        };
        let exponent_sign = if exp10 < 0 { '-' } else { '+' };
        return format!(
            "{sign}{mantissa}e{exponent_sign}{:02}",
            exp10.unsigned_abs()
        );
    }
    if exp10 < 0 {
        let zeros = "0".repeat(exp10.unsigned_abs() as usize - 1);
        return format!("{sign}0.{zeros}{digits}");
    }
    let integer_len = exp10 as usize + 1;
    if digits.len() <= integer_len {
        let zeros = "0".repeat(integer_len - digits.len());
        format!("{sign}{digits}{zeros}.0")
    } else {
        let (integer, fraction) = digits.split_at(integer_len);
        format!("{sign}{integer}.{fraction}")
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
    /// Ids of the learned rules the Tier 1 response answered (SPEC §6.3).
    pub rules_applied: &'a [String],
    /// `RULESET_VERSION` plus the number of changes to the active rule set.
    pub ruleset_version: u32,
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
    // `json!` turns NaN and infinity into `null` without a word, which would
    // seal a receipt that does not say what the decision rested on.
    debug_assert!(
        [
            input.amount_usd,
            input.decision.max_safe_spend_usd,
            input.config.caps.fair_usd,
            input.config.caps.good_usd,
            input.config.caps.very_good_usd,
            input.config.absolute_cap_usd,
        ]
        .iter()
        .all(|float| float.is_finite()),
        "evidence floats must be finite"
    );
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
        "ruleset_version": input.ruleset_version,
        "policy_config": input.config,
        "decision": {
            "decision": input.decision.kind.as_str(),
            "reasons": input.decision.reasons,
            "max_safe_spend": input.decision.max_safe_spend_usd,
        },
    })
}

/// The numeric confidence of every Tier 1 answer: `noul` for the live
/// wire shape, `probability_true` for the mock shape, `confidence` for
/// choice answers.
/// `noul` questions, `confidence` for choice questions.
fn confidences(answers: &Map<String, Value>) -> Value {
    let mut out = Map::new();
    for (question, answer) in answers {
        let number = answer
            .get("probability_true")
            .or_else(|| answer.get("noul"))
            .or_else(|| answer.get("confidence"))
            .filter(|value| value.is_number());
        if let Some(number) = number {
            out.insert(question.clone(), number.clone());
        }
    }
    Value::Object(out)
}

/// `GET /api/v1/receipt/:id` (SPEC §7.3): the receipt as JSON — the original
/// response plus the full evidence document and its creation time.
/// Read-only and unauthenticated. The HTML page for the same receipt is
/// `GET /receipt/:id`, rendered by `web::receipt_page`.
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
