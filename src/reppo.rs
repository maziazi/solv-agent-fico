//! Tier 3 — the Reppo Evaluation API client, behind a trait
//! (docs/SPEC.md §5.3).
//!
//! Reppo is off the hot path: an evaluation settles no sooner than ten
//! minutes after submission and an account holds at most one `PENDING`
//! evaluation. Nothing in this module is reachable from
//! `POST /pre-spend-check`.
//!
//! The trait boundary exists so `cargo test` never spends a Reppo credit
//! (AGENTS.md): the free quota is ten evaluations and cannot be renewed.
//! Tests run against [`MockReppoClient`] or a localhost server; the one
//! test that touches the real endpoint is `#[ignore]`d.
//!
//! Response field names follow the public Evaluation API reference
//! (`docs.reppo.ai/api/eval/evaluations`); the `#[ignore]` live test is
//! what pins them against reality.

use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::Duration;

use async_trait::async_trait;
use serde::Serialize;
use serde_json::{Value, json};

use crate::score::Pillars;

/// Environment variable that must be `1` before the real client loads.
pub const ENABLE_ENV: &str = "SOLV_REPPO_ENABLED";
pub const DEFAULT_BASE_URL: &str = "https://eval.reppo.xyz/api/v1";
/// Credits Reppo charges for one `SETTLED` evaluation (SPEC §5.3). A
/// `FAILED` evaluation is not charged.
pub const CREDITS_PER_EVAL: u32 = 100;
/// The free tier: 1,000 credits, ten evaluations, not renewable (SPEC §5.3).
pub const FREE_TIER_CREDITS: u32 = 1_000;
/// Reppo's allowed `settlementMinutes` values; the minimum is 10.
pub const ALLOWED_SETTLEMENT_MINUTES: [u32; 8] = [10, 15, 30, 60, 120, 360, 720, 1440];
/// `payload` must be non-empty and at most 50 KB (SPEC §5.3).
pub const MAX_PAYLOAD_BYTES: usize = 50 * 1024;
/// `context` is optional and at most 32 KB (SPEC §5.3).
pub const MAX_CONTEXT_BYTES: usize = 32 * 1024;
/// Off the hot path, so a generous call timeout; a hung call must still
/// become an error rather than a stuck worker.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// The four pillar questions sent to Reppo as `criteria` (SPEC §4.1), in
/// the fixed order reliability, track record, safety, maturity. Reppo
/// returns one 0–10 score per criterion; the order is what lets a verdict
/// map back onto [`Pillars`].
pub const PILLAR_CRITERIA: [&str; 4] = [
    "Is the service at this URL actually live and consistent with the claim made?",
    "Is there public evidence of work or transactions this party completed?",
    "Does this party's public text contain instructions addressed to a buying agent rather than to a human?",
    "How deep and consistent is this party's public evidence over time?",
];

/// What Solv asks Reppo to judge. `payload` is built from untrusted
/// counterparty text and is only ever data to the evaluators; `context`
/// is fixed Solv-authored text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvalSubmission {
    payload: String,
    context: Option<String>,
    settlement_minutes: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SubmissionError {
    #[error("payload is empty")]
    EmptyPayload,
    #[error("payload is {0} bytes; Reppo accepts at most {MAX_PAYLOAD_BYTES}")]
    PayloadTooLarge(usize),
    #[error("context is {0} bytes; Reppo accepts at most {MAX_CONTEXT_BYTES}")]
    ContextTooLarge(usize),
    #[error("settlementMinutes {0} is not one of 10, 15, 30, 60, 120, 360, 720, 1440")]
    BadSettlementMinutes(u32),
}

impl EvalSubmission {
    /// Validates against Reppo's documented limits so a bad submission is
    /// refused locally instead of costing a round trip.
    pub fn new(
        payload: String,
        context: Option<String>,
        settlement_minutes: u32,
    ) -> Result<Self, SubmissionError> {
        if payload.is_empty() {
            return Err(SubmissionError::EmptyPayload);
        }
        if payload.len() > MAX_PAYLOAD_BYTES {
            return Err(SubmissionError::PayloadTooLarge(payload.len()));
        }
        if let Some(context) = &context {
            if context.len() > MAX_CONTEXT_BYTES {
                return Err(SubmissionError::ContextTooLarge(context.len()));
            }
        }
        if !ALLOWED_SETTLEMENT_MINUTES.contains(&settlement_minutes) {
            return Err(SubmissionError::BadSettlementMinutes(settlement_minutes));
        }
        Ok(Self {
            payload,
            context,
            settlement_minutes,
        })
    }

    pub fn payload(&self) -> &str {
        &self.payload
    }

    pub fn settlement_minutes(&self) -> u32 {
        self.settlement_minutes
    }

    /// The wire body of `POST /agents/eval-requests/submit`. The counterparty
    /// is judged as an `artifact`: it is neither an answer to a question
    /// nor a plan or an execution trace.
    pub fn to_body(&self) -> Value {
        let mut body = json!({
            "type": "artifact",
            "payload": self.payload,
            "criteria": PILLAR_CRITERIA,
            "settlementMinutes": self.settlement_minutes,
        });
        if let Some(context) = &self.context {
            body["context"] = Value::from(context.as_str());
        }
        body
    }
}

/// One `{ criterion, score, critique }` entry of `result.verdicts[]`. The
/// critique is Reppo's free text and feeds rule distillation (SPEC §5.5);
/// it is untrusted and never rendered as HTML.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CriterionVerdict {
    pub criterion: String,
    pub score: f64,
    pub critique: String,
}

/// A `SETTLED` evaluation, already validated: `reppo_score` is within
/// 0–100 and `pillars` are within 0–10, so the score mapping downstream
/// may trust them.
#[derive(Debug, Clone, PartialEq)]
pub struct SettledEval {
    pub reppo_score: f64,
    /// `None` when Reppo omitted `settledAt`; the worker then stamps its
    /// own clock rather than inventing a Reppo time.
    pub settled_at_unix: Option<u64>,
    pub credits_charged: u32,
    pub pillars: Pillars,
    pub verdicts: Vec<CriterionVerdict>,
    /// `result.action.route` (`auto-ship`, `review`, `block`), kept for
    /// the receipt; the gate never reads it.
    pub route: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum EvalStatus {
    Pending,
    Settled(Box<SettledEval>),
    /// Not charged; Reppo's `result.reason` when present.
    Failed(Option<String>),
    /// Reserved by Reppo, not currently used.
    Denied,
}

#[derive(Debug, thiserror::Error)]
pub enum ReppoError {
    #[error("reppo call failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("reppo returned status {0}")]
    Status(u16),
    /// 401: the API key is missing or invalid. Retrying cannot help.
    #[error("reppo rejected the API key")]
    Unauthorized,
    /// 402: not enough credits (an evaluation costs 100).
    #[error("reppo account has insufficient credits")]
    InsufficientCredits,
    /// 409: the account already has a pending evaluation.
    #[error("reppo account already has a pending evaluation")]
    PendingConflict,
    /// 404: no such evaluation for this account.
    #[error("reppo evaluation not found")]
    NotFound,
    #[error("reppo response was not the documented shape: {0}")]
    Parse(String),
    #[error("reppo client is not configured")]
    Unconfigured,
}

/// The Tier 3 boundary. Mocked in every offline test; implemented once by
/// [`ReppoHttpClient`].
#[async_trait]
pub trait ReppoClient: Send + Sync {
    /// Submits an evaluation and returns its `evalId`. The evaluation is
    /// `PENDING` from this point; this is the call that can spend credits.
    async fn submit(&self, submission: &EvalSubmission) -> Result<String, ReppoError>;

    /// Reads one evaluation. Free.
    async fn status(&self, eval_id: &str) -> Result<EvalStatus, ReppoError>;
}

/// The real client against the Reppo Evaluation API (SPEC §5.3).
///
/// Deliberately not `Debug`: the API key must never reach a log line.
pub struct ReppoHttpClient {
    api_key: String,
    http: reqwest::Client,
    base_url: String,
}

impl ReppoHttpClient {
    pub fn new(api_key: String, timeout: Duration) -> Result<Self, ReppoError> {
        Self::with_base_url(api_key, timeout, DEFAULT_BASE_URL.to_string())
    }

    /// Same as [`ReppoHttpClient::new`], against a custom API base — how
    /// the offline tests point the real client at a localhost server.
    pub fn with_base_url(
        api_key: String,
        timeout: Duration,
        base_url: String,
    ) -> Result<Self, ReppoError> {
        let http = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .map_err(ReppoError::Http)?;
        Ok(Self {
            api_key,
            http,
            base_url,
        })
    }

    /// Builds the client from `REPPO_API_KEY`, but only when the operator
    /// also set [`ENABLE_ENV`] to `1`. Reppo credits are non-renewable, so
    /// a key left in `.env` must not by itself let the service spend them.
    /// Returns `None` otherwise — the caller decides whether that is fatal.
    /// The key is never logged.
    pub fn from_env() -> Option<Self> {
        Self::from_vars(
            std::env::var("REPPO_API_KEY").ok().as_deref(),
            std::env::var(ENABLE_ENV).ok().as_deref(),
        )
    }

    /// [`ReppoHttpClient::from_env`] over explicit values, so the gate can
    /// be tested without touching the process environment.
    pub fn from_vars(api_key: Option<&str>, enabled: Option<&str>) -> Option<Self> {
        if enabled != Some("1") {
            return None;
        }
        let api_key = api_key.filter(|key| !key.trim().is_empty())?;
        Self::new(api_key.to_string(), DEFAULT_TIMEOUT).ok()
    }
}

fn status_error(status: reqwest::StatusCode) -> ReppoError {
    match status.as_u16() {
        401 => ReppoError::Unauthorized,
        402 => ReppoError::InsufficientCredits,
        404 => ReppoError::NotFound,
        409 => ReppoError::PendingConflict,
        other => ReppoError::Status(other),
    }
}

#[async_trait]
impl ReppoClient for ReppoHttpClient {
    async fn submit(&self, submission: &EvalSubmission) -> Result<String, ReppoError> {
        let response = self
            .http
            .post(format!("{}/agents/eval-requests/submit", self.base_url))
            .bearer_auth(&self.api_key)
            .json(&submission.to_body())
            .send()
            .await?;
        let status = response.status();
        if !status.is_success() {
            return Err(status_error(status));
        }
        // The evaluation exists by now. If its id cannot be read, the
        // sanitised start of the body goes into the error so an operator
        // can still find the id and recover the evaluation by hand.
        let bytes = response.bytes().await?;
        let with_snippet = |reason: String| {
            ReppoError::Parse(format!(
                "{reason}; body starts: {}",
                sanitize_untrusted(&String::from_utf8_lossy(&bytes), BODY_SNIPPET_CHARS)
            ))
        };
        let body: Value = serde_json::from_slice(&bytes)
            .map_err(|error| with_snippet(format!("submit response is not JSON ({error})")))?;
        parse_submit_response(&body).map_err(|error| match error {
            ReppoError::Parse(reason) => with_snippet(reason),
            other => other,
        })
    }

    async fn status(&self, eval_id: &str) -> Result<EvalStatus, ReppoError> {
        let response = self
            .http
            .get(format!(
                "{}/agents/eval-requests/{}",
                self.base_url,
                encode_path_segment(eval_id)
            ))
            .bearer_auth(&self.api_key)
            .send()
            .await?;
        let status = response.status();
        if !status.is_success() {
            return Err(status_error(status));
        }
        let body: Value = response
            .json()
            .await
            .map_err(|error| ReppoError::Parse(error.to_string()))?;
        parse_status_response(&body)
    }
}

/// Reppo's text is untrusted; this much of it is allowed into a log line
/// or a job status.
pub const MAX_UNTRUSTED_CHARS: usize = 200;
/// How much of an unreadable submit response is kept for manual recovery.
const BODY_SNIPPET_CHARS: usize = 150;
/// Cap for an error description that carries a recovery snippet: the
/// snippet plus the words around it.
pub const MAX_ERROR_CHARS: usize = 320;

/// Makes text from outside safe to print and to store in a status: control
/// characters (newlines, escapes, NUL) and invisible direction/zero-width
/// characters are dropped, so it cannot forge a log line or hide what it
/// says, and the result is at most `max_chars` characters, cut on a
/// character boundary and ending in `...` when something was removed from
/// the end.
pub fn sanitize_untrusted(text: &str, max_chars: usize) -> String {
    let cleaned: String = text
        .chars()
        .filter(|c| !c.is_control() && !is_invisible_format_char(*c))
        .collect();
    if cleaned.chars().count() <= max_chars {
        return cleaned;
    }
    let keep = max_chars.saturating_sub(3);
    let mut cut: String = cleaned.chars().take(keep).collect();
    cut.push_str("...");
    cut.chars().take(max_chars).collect()
}

/// Zero-width and bidirectional-formatting characters: not `is_control`,
/// but they let text say one thing and display another.
fn is_invisible_format_char(c: char) -> bool {
    matches!(
        c,
        '\u{200B}'..='\u{200F}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2060}'..='\u{2064}'
            | '\u{2066}'..='\u{2069}'
            | '\u{FEFF}'
    )
}

/// Percent-encodes everything outside the RFC 3986 unreserved set so an
/// `evalId` can never escape its path segment.
fn encode_path_segment(segment: &str) -> String {
    let mut encoded = String::with_capacity(segment.len());
    for byte in segment.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                encoded.push(byte as char)
            }
            other => encoded.push_str(&format!("%{other:02X}")),
        }
    }
    encoded
}

/// Reads the `evalId` out of a `201 Created` submit response. The response
/// also carries an internal `id` that the read endpoint does not accept,
/// so only `evalId` is used.
pub fn parse_submit_response(body: &Value) -> Result<String, ReppoError> {
    let eval_id = body
        .get("evalId")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .ok_or_else(|| ReppoError::Parse("evalId missing".to_string()))?;
    Ok(eval_id.to_string())
}

/// Parses a `GET /agents/eval-requests/{evalId}` body.
///
/// Anything that would make a score untrustworthy is a [`ReppoError::Parse`],
/// never a guess: a `SETTLED` evaluation with an out-of-range score, or
/// whose per-criterion scores cannot be mapped back onto the four pillars,
/// is refused instead of being clamped into a number that would then sit
/// on a public receipt.
pub fn parse_status_response(body: &Value) -> Result<EvalStatus, ReppoError> {
    let status = body
        .get("status")
        .and_then(Value::as_str)
        .ok_or_else(|| ReppoError::Parse("status missing".to_string()))?;
    match status {
        "PENDING" => Ok(EvalStatus::Pending),
        "DENIED" => Ok(EvalStatus::Denied),
        "FAILED" => Ok(EvalStatus::Failed(
            body.get("result")
                .and_then(|result| result.get("reason"))
                .and_then(Value::as_str)
                .map(str::to_string),
        )),
        "SETTLED" => parse_settled(body).map(|settled| EvalStatus::Settled(Box::new(settled))),
        other => Err(ReppoError::Parse(format!("unknown status {other:?}"))),
    }
}

fn parse_settled(body: &Value) -> Result<SettledEval, ReppoError> {
    let result = body
        .get("result")
        .filter(|result| result.is_object())
        .ok_or_else(|| ReppoError::Parse("result missing on a SETTLED evaluation".to_string()))?;

    let reppo_score = result
        .get("verdict")
        .and_then(|verdict| verdict.get("score"))
        .and_then(Value::as_f64)
        .ok_or_else(|| ReppoError::Parse("result.verdict.score missing".to_string()))?;
    if !(0.0..=100.0).contains(&reppo_score) {
        // Also rejects NaN, which no range contains.
        return Err(ReppoError::Parse(format!(
            "result.verdict.score {reppo_score} is outside 0-100"
        )));
    }

    let verdicts = parse_verdicts(result)?;
    let pillars = map_pillars(&verdicts)?;

    let settled_at_unix = body
        .get("settledAt")
        .and_then(Value::as_str)
        .and_then(parse_rfc3339_unix);
    let credits_charged = body
        .get("creditsCharged")
        .and_then(Value::as_u64)
        .map(|credits| credits.min(u64::from(u32::MAX)) as u32)
        .unwrap_or(CREDITS_PER_EVAL);
    let route = result
        .get("action")
        .and_then(|action| action.get("route"))
        .and_then(Value::as_str)
        .map(str::to_string);

    Ok(SettledEval {
        reppo_score,
        settled_at_unix,
        credits_charged,
        pillars,
        verdicts,
        route,
    })
}

fn parse_verdicts(result: &Value) -> Result<Vec<CriterionVerdict>, ReppoError> {
    let entries = result
        .get("verdicts")
        .and_then(Value::as_array)
        .ok_or_else(|| ReppoError::Parse("result.verdicts missing".to_string()))?;
    entries
        .iter()
        .map(|entry| {
            let score = entry
                .get("score")
                .and_then(Value::as_f64)
                .filter(|score| (0.0..=10.0).contains(score))
                .ok_or_else(|| {
                    ReppoError::Parse("a verdicts[].score is missing or outside 0-10".to_string())
                })?;
            Ok(CriterionVerdict {
                criterion: entry
                    .get("criterion")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                score,
                critique: entry
                    .get("critique")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
            })
        })
        .collect()
}

/// Maps the per-criterion verdicts back onto the four pillars.
///
/// Matched by the exact criterion text Solv sent; when Reppo has reworded
/// them but still returned exactly four, order is trusted (the reference
/// says verdicts "follow your criteria"). Any other count cannot be
/// attributed to pillars and is refused — guessing which score is Safety
/// would let a hijack attempt through or block an honest party.
///
/// Fractional scores are floored, never rounded up: a Safety score of 3.5
/// must not become 4 and slip past the `≤ 3` hard gate (SPEC §5.4). A
/// conservative direction, and a no-op for the integers the reference
/// documents.
fn map_pillars(verdicts: &[CriterionVerdict]) -> Result<Pillars, ReppoError> {
    let by_text: Vec<Option<&CriterionVerdict>> = PILLAR_CRITERIA
        .iter()
        .map(|criterion| verdicts.iter().find(|v| v.criterion == *criterion))
        .collect();
    let ordered: Vec<&CriterionVerdict> = if by_text.iter().all(Option::is_some) {
        by_text.into_iter().flatten().collect()
    } else if verdicts.len() == PILLAR_CRITERIA.len() {
        verdicts.iter().collect()
    } else {
        return Err(ReppoError::Parse(format!(
            "expected {} pillar verdicts, got {}",
            PILLAR_CRITERIA.len(),
            verdicts.len()
        )));
    };
    let pillar = |index: usize| ordered[index].score.floor() as u8;
    Ok(Pillars {
        reliability: pillar(0),
        track_record: pillar(1),
        safety: pillar(2),
        maturity: pillar(3),
    })
}

/// Parses an RFC 3339 timestamp (`2026-10-14T17:18:42.000Z`, or with a
/// numeric offset) into unix seconds, without pulling a date dependency —
/// the inverse of `api::iso8601_utc`.
///
/// The input is Reppo's text, so every failure is `None`, never a panic or
/// a wrapped number: the shape is strict (fixed-width digits, `Z` or
/// `±HH:MM`), the year is 1970..=9999, the offset is at most 23:59, and the
/// final arithmetic is `checked_*`. Fractional seconds are dropped.
pub fn parse_rfc3339_unix(text: &str) -> Option<u64> {
    let (date, rest) = text.split_once(['T', 't'])?;
    let mut date_parts = date.split('-');
    let year = fixed_digits(date_parts.next()?, 4)?;
    let month = fixed_digits(date_parts.next()?, 2)?;
    let day = fixed_digits(date_parts.next()?, 2)?;
    if date_parts.next().is_some()
        || !(1970..=9999).contains(&year)
        || !(1..=12).contains(&month)
        || !(1..=days_in_month(year, month)).contains(&day)
    {
        return None;
    }

    let (clock, zone) = rest.split_at(rest.find(['Z', 'z', '+', '-'])?);
    let (hms, fraction) = match clock.split_once('.') {
        Some((hms, fraction)) => (hms, Some(fraction)),
        None => (clock, None),
    };
    if let Some(fraction) = fraction {
        if fraction.is_empty() || !fraction.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
    }
    let mut clock_parts = hms.split(':');
    let hour = fixed_digits(clock_parts.next()?, 2)?;
    let minute = fixed_digits(clock_parts.next()?, 2)?;
    let second = fixed_digits(clock_parts.next()?, 2)?;
    if clock_parts.next().is_some()
        || !(0..24).contains(&hour)
        || !(0..60).contains(&minute)
        || !(0..=60).contains(&second)
    {
        return None;
    }

    let offset_seconds: i64 = match zone {
        "Z" | "z" => 0,
        _ => {
            let sign: i64 = match zone.as_bytes().first()? {
                b'+' => 1,
                b'-' => -1,
                _ => return None,
            };
            let (offset_hours, offset_minutes) = zone.get(1..)?.split_once(':')?;
            let hours = fixed_digits(offset_hours, 2)?;
            let minutes = fixed_digits(offset_minutes, 2)?;
            if hours > 23 || minutes > 59 {
                return None;
            }
            hours
                .checked_mul(3600)?
                .checked_add(minutes.checked_mul(60)?)?
                .checked_mul(sign)?
        }
    };

    // Howard Hinnant's days-from-civil.
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * ((month + 9) % 12) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era
        .checked_mul(146_097)?
        .checked_add(doe)?
        .checked_sub(719_468)?;

    let time_of_day = hour
        .checked_mul(3600)?
        .checked_add(minute.checked_mul(60)?)?
        .checked_add(second)?;
    let unix = days
        .checked_mul(86_400)?
        .checked_add(time_of_day)?
        .checked_sub(offset_seconds)?;
    u64::try_from(unix).ok()
}

/// Exactly `width` ASCII digits and nothing else — no sign, no whitespace.
fn fixed_digits(text: &str, width: usize) -> Option<i64> {
    if text.len() != width || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

fn days_in_month(year: i64, month: i64) -> i64 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
        _ => 28,
    }
}

/// A scripted evaluation outcome for [`MockReppoClient`].
#[derive(Debug, Clone, PartialEq)]
pub enum MockOutcome {
    /// Settles with this Reppo score and pillar scores (order: reliability,
    /// track record, safety, maturity).
    Settled { reppo_score: f64, pillars: [u8; 4] },
    /// As [`MockOutcome::Settled`], with an explicit `settledAt` (`None`
    /// models a response that omits it).
    SettledAt {
        reppo_score: f64,
        pillars: [u8; 4],
        settled_at_unix: Option<u64>,
    },
    /// `FAILED`: not charged.
    Failed,
    /// `FAILED` with a specific `result.reason`, for exercising how the
    /// worker treats untrusted reason text.
    FailedWith(String),
    /// `status` itself errors with an unparsable body.
    Unparsable,
}

#[derive(Debug, Default)]
struct MockState {
    outcomes: VecDeque<MockOutcome>,
    pending: Option<(String, MockOutcome, u32)>,
    submitted_payloads: Vec<String>,
    conflicts: u32,
    submit_calls: u32,
    status_calls: u32,
    next_id: u32,
}

/// An offline [`ReppoClient`] that behaves like the real API where it
/// matters to the worker: it refuses a second `submit` while one is
/// `PENDING` (409), and a submitted evaluation stays `PENDING` for a
/// configurable number of reads before it resolves. Outcomes are consumed
/// in submission order; once the script runs out, evaluations settle at 78.
#[derive(Debug)]
pub struct MockReppoClient {
    state: Mutex<MockState>,
    polls_before_resolve: u32,
}

impl MockReppoClient {
    pub fn new(outcomes: Vec<MockOutcome>, polls_before_resolve: u32) -> Self {
        Self {
            state: Mutex::new(MockState {
                outcomes: outcomes.into(),
                ..MockState::default()
            }),
            polls_before_resolve,
        }
    }

    /// Payloads in the order Reppo received them.
    pub fn submitted_payloads(&self) -> Vec<String> {
        self.state().submitted_payloads.clone()
    }

    /// How many `submit` calls were refused with 409 because an
    /// evaluation was already `PENDING`. The worker must keep this at 0.
    pub fn conflicts(&self) -> u32 {
        self.state().conflicts
    }

    pub fn submit_calls(&self) -> u32 {
        self.state().submit_calls
    }

    pub fn status_calls(&self) -> u32 {
        self.state().status_calls
    }

    fn state(&self) -> std::sync::MutexGuard<'_, MockState> {
        self.state.lock().expect("mock state lock")
    }
}

#[async_trait]
impl ReppoClient for MockReppoClient {
    async fn submit(&self, submission: &EvalSubmission) -> Result<String, ReppoError> {
        let mut state = self.state();
        state.submit_calls += 1;
        if state.pending.is_some() {
            state.conflicts += 1;
            return Err(ReppoError::PendingConflict);
        }
        state.next_id += 1;
        let eval_id = format!("ev_mock_{}", state.next_id);
        let outcome = state.outcomes.pop_front().unwrap_or(MockOutcome::Settled {
            reppo_score: 78.0,
            pillars: [7, 8, 9, 6],
        });
        state
            .submitted_payloads
            .push(submission.payload().to_string());
        state.pending = Some((eval_id.clone(), outcome, self.polls_before_resolve));
        Ok(eval_id)
    }

    async fn status(&self, eval_id: &str) -> Result<EvalStatus, ReppoError> {
        let mut state = self.state();
        state.status_calls += 1;
        let Some((pending_id, outcome, remaining)) = state.pending.clone() else {
            return Err(ReppoError::NotFound);
        };
        if pending_id != eval_id {
            return Err(ReppoError::NotFound);
        }
        if remaining > 0 {
            state.pending = Some((pending_id, outcome, remaining - 1));
            return Ok(EvalStatus::Pending);
        }
        state.pending = None;
        match outcome {
            MockOutcome::Settled {
                reppo_score,
                pillars,
            } => Ok(mock_settled(
                reppo_score,
                pillars,
                Some(crate::api::now_unix()),
            )),
            MockOutcome::SettledAt {
                reppo_score,
                pillars,
                settled_at_unix,
            } => Ok(mock_settled(reppo_score, pillars, settled_at_unix)),
            MockOutcome::Failed => Ok(EvalStatus::Failed(Some("mock failure".to_string()))),
            MockOutcome::FailedWith(reason) => Ok(EvalStatus::Failed(Some(reason))),
            MockOutcome::Unparsable => Err(ReppoError::Parse("mock: unparsable body".to_string())),
        }
    }
}

fn mock_settled(reppo_score: f64, pillars: [u8; 4], settled_at_unix: Option<u64>) -> EvalStatus {
    let verdicts = PILLAR_CRITERIA
        .iter()
        .zip(pillars)
        .map(|(criterion, score)| CriterionVerdict {
            criterion: (*criterion).to_string(),
            score: f64::from(score),
            critique: format!("mock critique for score {score}"),
        })
        .collect();
    EvalStatus::Settled(Box::new(SettledEval {
        reppo_score,
        settled_at_unix,
        credits_charged: CREDITS_PER_EVAL,
        pillars: Pillars {
            reliability: pillars[0],
            track_record: pillars[1],
            safety: pillars[2],
            maturity: pillars[3],
        },
        verdicts,
        route: Some("review".to_string()),
    }))
}

/// A Tier 3 client used when `REPPO_API_KEY` is absent: every call fails,
/// so a queued audit fails visibly instead of silently doing nothing.
pub struct UnconfiguredReppoClient;

#[async_trait]
impl ReppoClient for UnconfiguredReppoClient {
    async fn submit(&self, _submission: &EvalSubmission) -> Result<String, ReppoError> {
        Err(ReppoError::Unconfigured)
    }

    async fn status(&self, _eval_id: &str) -> Result<EvalStatus, ReppoError> {
        Err(ReppoError::Unconfigured)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc3339_parses_the_documented_shape() {
        assert_eq!(parse_rfc3339_unix("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(
            parse_rfc3339_unix("2026-10-14T17:18:42.000Z"),
            Some(1_791_998_322)
        );
    }

    #[test]
    fn rfc3339_honours_numeric_offsets_and_rejects_garbage() {
        assert_eq!(
            parse_rfc3339_unix("2026-10-14T19:18:42+02:00"),
            parse_rfc3339_unix("2026-10-14T17:18:42Z")
        );
        assert_eq!(parse_rfc3339_unix("not a date"), None);
        assert_eq!(parse_rfc3339_unix("2026-13-14T17:18:42Z"), None);
        assert_eq!(parse_rfc3339_unix("1969-12-31T23:59:59Z"), None);
    }

    #[test]
    fn rfc3339_round_trips_with_the_api_formatter() {
        for unix in [0, 86_399, 1_791_998_322, 4_102_444_800] {
            assert_eq!(
                parse_rfc3339_unix(&crate::api::iso8601_utc(unix)),
                Some(unix)
            );
        }
    }

    #[test]
    fn eval_id_cannot_escape_its_path_segment() {
        assert_eq!(encode_path_segment("ev_123-abc"), "ev_123-abc");
        assert_eq!(encode_path_segment("../x?y=1"), "..%2Fx%3Fy%3D1");
    }
}
