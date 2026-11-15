//! The hot-path endpoint: `POST /api/v1/pre-spend-check` (docs/SPEC.md §7.1).
//!
//! Orchestration only — the decision itself is the deterministic policy
//! gate (SPEC §5.4). Hot-path rules enforced here (SPEC §2.1): no disk
//! I/O, one Tier 1 call, Tier 2 only on escalation, Reppo never.
//!
//! Every decision is sealed into a receipt before it is returned (SPEC §3.3
//! G4): the insert is an in-memory map write and the snapshot is only
//! requested, so no disk I/O touches the hot path.

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
use crate::receipt::{self, EvidenceInput, RULESET_VERSION, Tier1Evidence};
use crate::score::{self, Pillars};
use crate::state::{Profile, Receipt};
use crate::tier1::{ClefState, KnownProfile, MODEL_NAME, standard_request};

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
/// Sortable by time, collision-free in practice; the receipt store still
/// refuses a duplicate rather than trust that.
pub fn new_receipt_id() -> String {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let n = RECEIPT_COUNTER.fetch_add(1, Ordering::Relaxed);
    // The counter is kept to six hex digits so the id length is stable;
    // wraparound alone cannot collide because the clock has moved on.
    format!(
        "slv_{:010x}{:08x}{:06x}",
        elapsed.as_secs(),
        elapsed.subsec_nanos(),
        n % 0x100_0000
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
    /// ISO-8601 UTC, the shape SPEC §7.1 pins on receipts.
    settled_at: String,
}

/// Formats a unix timestamp as ISO-8601 UTC without pulling a date
/// dependency: civil-from-days (Howard Hinnant's algorithm).
pub fn iso8601_utc(unix_seconds: u64) -> String {
    let days = (unix_seconds / 86_400) as i64;
    let secs_of_day = unix_seconds % 86_400;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        secs_of_day / 3600,
        (secs_of_day % 3600) / 60,
        secs_of_day % 60
    )
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
    ruleset_version: u32,
    evidence_sha256: String,
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

    let gate_request = PreSpendRequest {
        counterparty_name: request.counterparty_name.clone(),
        target_url: request.target_url.clone(),
        amount_usd: request.amount_usd,
        service_description: request.service_description.clone(),
    };

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
    let (tier1_signals, tier1_answers) =
        match state.tier1().ask(&standard_request(clef_state)).await {
            Ok(response) => (response.to_signals(), Some(response.answers)),
            Err(_) => (crate::policy::Tier1Signals::unavailable(), None),
        };
    let tier1_latency = tier1_started.elapsed().as_millis() as u64;
    // The request struct is consumed by the gate; everything before this
    // point worked from clones of the same fields.
    drop(request);

    // Escalation is deterministic (SPEC §5.1). Tier 2 itself arrives with
    // PR-07; until then an escalated request records Tier 2 as unavailable
    // and the gate fails closed where the verdict requires it.
    let escalated = needs_escalation(&gate_request, &tier1_signals, tier, &config);
    let tier2_signals = if escalated {
        Some(Tier2Signals {
            outcome: Tier2Outcome::Unavailable,
        })
    } else {
        None
    };

    let decision = decide(
        &gate_request,
        profile.as_ref(),
        now,
        started.elapsed().as_millis() as u64,
        &tier1_signals,
        tier2_signals.as_ref(),
        &[],
        &config,
    );
    let total_ms = started.elapsed().as_millis() as u64;

    let receipt_id = new_receipt_id();
    // `tier_used` names the tiers that actually ran. Until PR-07 wires the
    // real Tier 2 client, only Tier 1 ever runs — an escalated request is
    // held by the gate (SOLV-SYSTEM-003), never silently approved.
    let tier_used = vec!["clef-flash"];

    let evidence = receipt::build_evidence(&EvidenceInput {
        counterparty_id: &gate_request.counterparty_name,
        target_url: &gate_request.target_url,
        amount_usd: gate_request.amount_usd,
        service_description: &gate_request.service_description,
        settled,
        effective,
        tier1: Tier1Evidence {
            model: MODEL_NAME,
            answers: tier1_answers.as_ref(),
            latency_ms: tier1_latency,
        },
        tier2: None,
        rules_applied: &[],
        config: &config,
        decision: &decision,
    });
    let evidence_sha256 = receipt::evidence_sha256(&evidence);

    let body = PreSpendCheckResponse {
        receipt_url: format!("/receipt/{receipt_id}"),
        receipt_id: receipt_id.clone(),
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
            settled_at: iso8601_utc(s.settled_at_unix),
        }),
        tier_used,
        latency_ms: LatencyMs {
            state: state_latency,
            tier1: tier1_latency,
            tier2: None,
            total: total_ms,
        },
        reasons: decision.reasons,
        ruleset_version: RULESET_VERSION,
        evidence_sha256: evidence_sha256.clone(),
    };
    let response =
        serde_json::to_value(&body).expect("serialising the response struct cannot fail");

    // G4: no decision leaves without a stored receipt. An id collision is
    // astronomically unlikely, but a duplicate must never overwrite an
    // existing receipt, so it fails the request instead of the audit trail.
    let sealed = state.store().insert_receipt(
        receipt_id,
        Receipt {
            decision: body.decision.to_string(),
            amount_usd: gate_request.amount_usd,
            created_at_unix: now,
            evidence_sha256,
            evidence,
            response: response.clone(),
        },
    );
    if !sealed {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "receipt id collision; retry the request" })),
        )
            .into_response();
    }
    state.store().request_snapshot();

    (StatusCode::OK, Json(response)).into_response()
}
