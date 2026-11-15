//! The hot-path endpoint: `POST /api/v1/pre-spend-check` (docs/SPEC.md §7.1).
//!
//! Orchestration only — the decision itself is the deterministic policy
//! gate (SPEC §5.4). Hot-path rules enforced here (SPEC §2.1): no disk
//! I/O, one Tier 1 call, Tier 2 only on escalation, Reppo never.
//!
//! Deferred to PR-08 by the PR map: `ruleset_version`, `evidence_sha256`,
//! and durable receipts — the receipt id returned today is unique but the
//! stored, hash-carrying receipt is PR-08's capability.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::policy::{PreSpendRequest, Tier2Outcome, Tier2Signals, decide, needs_escalation};
use crate::reason::Reason;
use crate::score::{self, Pillars};
use crate::state::Profile;
use crate::tier1::{ClefState, KnownProfile, standard_request};

/// Descriptions over 50 KB are rejected outright (SPEC §8.2); between
/// 1,500 characters and this limit they escalate instead.
const MAX_DESCRIPTION_BYTES: usize = 50 * 1024;

/// Monotonic per-process receipt-id counter, paired with the clock so ids
/// stay unique across restarts.
static RECEIPT_COUNTER: AtomicU64 = AtomicU64::new(0);

pub fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Unique receipt id: `slv_` + seconds + nanos + per-process counter.
/// Sortable by time, collision-free in practice; PR-08 stores what it
/// names.
pub fn new_receipt_id() -> String {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let n = RECEIPT_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!(
        "slv_{:010x}{:08x}{:04x}",
        elapsed.as_secs(),
        elapsed.subsec_nanos(),
        n & 0xffff
    )
}

#[derive(Debug, Deserialize)]
pub struct PreSpendCheckRequest {
    pub counterparty_name: String,
    pub target_url: String,
    pub amount_usd: f64,
    pub service_description: String,
}

#[derive(Debug, Serialize)]
struct ScoreSource {
    provider: &'static str,
    eval_id: String,
    settled_at_unix: u64,
}

#[derive(Debug, Serialize)]
struct LatencyMs {
    state: u64,
    tier1: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    tier2: Option<u64>,
    total: u64,
}

#[derive(Debug, Serialize)]
struct PreSpendCheckResponse {
    receipt_id: String,
    decision: &'static str,
    fico_score: Option<u16>,
    fico_raw: Option<u16>,
    decay: Option<u32>,
    score_age_days: Option<u32>,
    tier: String,
    max_safe_spend: f64,
    pillars: Option<Pillars>,
    score_source: Option<ScoreSource>,
    tier_used: Vec<&'static str>,
    latency_ms: LatencyMs,
    reasons: Vec<Reason>,
    receipt_url: String,
}

pub async fn pre_spend_check(
    State(state): State<AppState>,
    Json(request): Json<PreSpendCheckRequest>,
) -> Response {
    let started = std::time::Instant::now();

    if request.service_description.len() > MAX_DESCRIPTION_BYTES {
        return (
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(serde_json::json!({
                "error": "service_description exceeds 50 KB and is rejected"
            })),
        )
            .into_response();
    }

    // State read: in-process, budgeted at ≤ 1 ms p95 (SPEC §2.1).
    let state_started = std::time::Instant::now();
    let profile: Option<Profile> = state.store().profile(&request.counterparty_name);
    let state_latency = state_started.elapsed().as_millis() as u64;

    let now = now_unix();
    let settled = profile.as_ref().and_then(|p| p.score.as_ref());
    let effective = settled.and_then(|s| score::effective_score(s, now));
    let tier = score::tier_for(effective.map(|s| s.fico_effective));

    // Tier 1 — the single reflex call.
    let config = crate::policy::PolicyConfig::default();
    let clef_state = ClefState {
        counterparty_name: request.counterparty_name.clone(),
        target_url: request.target_url.clone(),
        amount_usd: request.amount_usd,
        service_description: request.service_description.clone(),
        known_profile: effective.map(|effective| KnownProfile {
            fico_effective: effective.fico_effective,
            tier: tier.as_str().to_string(),
            age_days: effective.age_days,
            pillars: settled.expect("effective implies settled").pillars,
        }),
    };
    let tier1_started = std::time::Instant::now();
    let tier1_signals = match state.tier1().ask(&standard_request(clef_state)).await {
        Ok(response) => response.to_signals(),
        Err(_) => crate::policy::Tier1Signals::unavailable(),
    };
    let tier1_latency = tier1_started.elapsed().as_millis() as u64;

    // Escalation is deterministic (SPEC §5.1). Tier 2 itself arrives with
    // PR-07; until then an escalated request records Tier 2 as unavailable
    // and the gate fails closed where the verdict requires it.
    let escalated = needs_escalation(
        &PreSpendRequest {
            counterparty_name: request.counterparty_name.clone(),
            target_url: request.target_url.clone(),
            amount_usd: request.amount_usd,
            service_description: request.service_description.clone(),
        },
        &tier1_signals,
        tier,
        &config,
    );
    let tier2_signals = if escalated {
        Some(Tier2Signals {
            outcome: Tier2Outcome::Unavailable,
        })
    } else {
        None
    };

    let total_ms = started.elapsed().as_millis() as u64;
    let decision = decide(
        &PreSpendRequest {
            counterparty_name: request.counterparty_name,
            target_url: request.target_url,
            amount_usd: request.amount_usd,
            service_description: request.service_description,
        },
        profile.as_ref(),
        now,
        total_ms,
        &tier1_signals,
        tier2_signals.as_ref(),
        &[],
        &config,
    );

    let receipt_id = new_receipt_id();
    let mut tier_used = vec!["clef-flash"];
    if escalated {
        tier_used.push("serv-reasoning");
    }

    let body = PreSpendCheckResponse {
        receipt_url: format!("/receipt/{receipt_id}"),
        receipt_id,
        decision: decision.kind.as_str(),
        fico_score: effective.map(|s| s.fico_effective),
        fico_raw: effective.map(|s| s.fico_raw),
        decay: effective.map(|s| s.decay),
        score_age_days: effective.map(|s| s.age_days),
        tier: decision.tier.as_str().to_string(),
        max_safe_spend: decision.max_safe_spend_usd,
        pillars: settled.map(|s| s.pillars),
        score_source: settled.map(|s| ScoreSource {
            provider: "reppo",
            eval_id: s.eval_id.clone(),
            settled_at_unix: s.settled_at_unix,
        }),
        tier_used,
        latency_ms: LatencyMs {
            state: state_latency,
            tier1: tier1_latency,
            tier2: None,
            total: total_ms,
        },
        reasons: decision.reasons,
    };

    (StatusCode::OK, Json(body)).into_response()
}
