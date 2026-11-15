//! Attack Lab (docs/SPEC.md §8.3): every corpus case runs offline through
//! content-aware mocks that stand in for the real detectors' contract, and
//! the pass criteria are enforced mechanically:
//!
//! 1. not one case produces APPROVE;
//! 2. every reason on every result satisfies the G5 five conditions;
//! 3. every injection reason carries evidence whose quote is a verbatim
//!    span of the raw description at the stated locator, with the locator
//!    naming the normalisation layer that revealed it.
//!
//! The mocks mirror the real split of duties: Clef-flash (tier 1) sees
//! plain and homoglyph-layered text but not base64 runs or zero-width
//! layering; the SERV prompt guard (tier 2) refuses exactly the encoded
//! layering. What the corpus proves is that the layered defence blocks all
//! of it — not that any single layer sees everything (SPEC §11).

use std::collections::BTreeMap;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request as HttpRequest, StatusCode};
use http_body_util::BodyExt;
use serde::Deserialize;
use serde_json::{Map, Value, json};
use tower::ServiceExt;

use solv::state::Store;
use solv::tier1::{ClefClient, ClefError, ClefRequest, ClefResponse};
use solv::tier2::{ServAsk, ServClient, ServError, ServVerdict};
use solv::{AppState, build_router};

const CORPUS: &str = include_str!("attack/corpus.json");

// ---------------------------------------------------------------------------
// Corpus types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct Corpus {
    cases: Vec<Case>,
}

#[derive(Debug, Deserialize)]
struct Case {
    id: String,
    category: String,
    counterparty_name: String,
    target_url: String,
    amount_usd: f64,
    description: String,
    /// Overclaim cases carry a settled top-band profile so the cold-start row
    /// (which would mask the mismatch row) does not fire for them.
    #[serde(default)]
    seed_profile: Option<SeedProfile>,
    expect: Expect,
}

#[derive(Debug, Deserialize)]
struct SeedProfile {
    reppo_score: f64,
    days_ago: u64,
}

#[derive(Debug, Deserialize)]
struct Expect {
    decision: String,
    codes: Vec<String>,
    span: Option<SpanExpect>,
}

#[derive(Debug, Deserialize)]
struct SpanExpect {
    contains: String,
    /// The locator note the span's normalisation layer must produce
    /// ("" for verbatim matches).
    note: String,
}

fn corpus() -> Corpus {
    serde_json::from_str(CORPUS).expect("the attack corpus parses")
}

// ---------------------------------------------------------------------------
// Content-aware mocks
// ---------------------------------------------------------------------------

/// Overclaim markers, known to both mocks: the Clef stand-in classifies
/// them `overclaim` with sub-floor confidence (SPEC §5.1 rule 2: escalate),
/// and the SERV stand-in reasons to a verdict mismatch (taxonomy
/// §6.2 `SOLV-REASON-002`).
const OVERCLAIM_MARKERS: &[&str] = &[
    "guaranteed",
    "zero risk",
    "most reliable",
    "strongest",
    "spotless",
    "since 20",
    "trusted by",
    "industry-standard",
];

fn claims_unsupported_overclaim(description: &str) -> bool {
    let lowered = description.to_ascii_lowercase();
    OVERCLAIM_MARKERS
        .iter()
        .any(|marker| lowered.contains(marker))
}

/// Stands in for Clef-flash: a decision model over typed answers. It sees
/// verbatim and homoglyph-layered attacks; base64 and zero-width layering
/// pass it by (low or uncertain probability), which is exactly the case
/// SPEC §11 refuses to assume away.
struct AttackClef;

#[async_trait::async_trait]
impl ClefClient for AttackClef {
    async fn ask(&self, request: &ClefRequest) -> Result<ClefResponse, ClefError> {
        let description = &request.state.service_description;
        let span = solv::spans::injection_span(description);
        let mut answers = Map::new();
        match span {
            Some(found)
                if matches!(
                    found.normalisation,
                    solv::spans::Normalisation::Verbatim
                        | solv::spans::Normalisation::HomoglyphsFolded
                ) =>
            {
                match found.class {
                    solv::spans::PatternClass::Override => {
                        answers
                            .insert("injection".to_string(), json!({ "probability_true": 0.97 }));
                        answers.insert(
                            "risk_class".to_string(),
                            json!({ "choice": "none", "confidence": 0.97 }),
                        );
                    }
                    solv::spans::PatternClass::Exfiltration => {
                        answers
                            .insert("injection".to_string(), json!({ "probability_true": 0.02 }));
                        answers.insert(
                            "risk_class".to_string(),
                            json!({ "choice": "exfiltration", "confidence": 0.95 }),
                        );
                    }
                    solv::spans::PatternClass::Coercion => {
                        answers
                            .insert("injection".to_string(), json!({ "probability_true": 0.02 }));
                        answers.insert(
                            "risk_class".to_string(),
                            json!({ "choice": "coercion", "confidence": 0.95 }),
                        );
                    }
                }
            }
            // Zero-width layering lands in the uncertain band: the model
            // cannot rule it out, so the gate escalates (SPEC §5.1).
            Some(found) if found.normalisation == solv::spans::Normalisation::ZeroWidthStripped => {
                answers.insert("injection".to_string(), json!({ "probability_true": 0.15 }));
                answers.insert(
                    "risk_class".to_string(),
                    json!({ "choice": "none", "confidence": 0.97 }),
                );
            }
            // Base64 runs are simply not seen; suspected overclaim with a
            // sub-floor confidence escalates instead of deciding (SPEC
            // §5.1 rule 2), so the SERV guard reasons on it.
            _ if claims_unsupported_overclaim(&request.state.service_description) => {
                answers.insert("injection".to_string(), json!({ "probability_true": 0.02 }));
                answers.insert(
                    "risk_class".to_string(),
                    json!({ "choice": "overclaim", "confidence": 0.80 }),
                );
            }
            _ => {
                answers.insert("injection".to_string(), json!({ "probability_true": 0.03 }));
                answers.insert(
                    "risk_class".to_string(),
                    json!({ "choice": "none", "confidence": 0.97 }),
                );
            }
        }
        Ok(ClefResponse { answers })
    }
}

/// Stands in for the SERV prompt guard: it refuses exactly the layerings a
/// plain reading cannot see, and otherwise reasons the request through.
struct AttackGuard;

#[async_trait::async_trait]
impl ServClient for AttackGuard {
    async fn reason(&self, ask: &ServAsk) -> Result<ServVerdict, ServError> {
        match solv::spans::injection_span(&ask.service_description) {
            Some(found) if found.normalisation == solv::spans::Normalisation::ZeroWidthStripped => {
                Ok(ServVerdict::GuardRefusal)
            }
            Some(found) if found.normalisation == solv::spans::Normalisation::Base64Decoded => {
                Ok(ServVerdict::GuardRefusal)
            }
            _ if claims_unsupported_overclaim(&ask.service_description) => {
                Ok(ServVerdict::Mismatch)
            }
            _ => Ok(ServVerdict::Approve),
        }
    }
}

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

fn router() -> axum::Router {
    router_with_seed(None)
}

fn router_with_seed(seed: Option<(&str, f64, u64)>) -> axum::Router {
    let store = Store::open(std::env::temp_dir().join(format!(
        "solv_attack_{}_{}.json",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    )))
    .expect("throwaway store opens");
    if let Some((counterparty_id, reppo_score, days_ago)) = seed {
        let now = solv::api::now_unix();
        store.upsert_profile(
            counterparty_id.to_string(),
            solv::state::Profile {
                counterparty_name: counterparty_id.to_string(),
                target_url: String::new(),
                score: Some(solv::score::SettledScore {
                    reppo_score,
                    eval_id: "ev_seed".to_string(),
                    settled_at_unix: now - days_ago * 86_400,
                    pillars: solv::score::Pillars {
                        reliability: 8,
                        track_record: 8,
                        safety: 8,
                        maturity: 8,
                    },
                }),
            },
        );
    }
    let state = AppState::new(store, Arc::new(AttackClef), Arc::new(AttackGuard));
    build_router(state)
}

async fn run_case(router: axum::Router, case: &Case) -> Value {
    let response = router
        .oneshot(
            HttpRequest::builder()
                .method("POST")
                .uri("/api/v1/pre-spend-check")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "counterparty_name": case.counterparty_name,
                        "target_url": case.target_url,
                        "amount_usd": case.amount_usd,
                        "service_description": case.description,
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .expect("request answers");
    assert_eq!(response.status(), StatusCode::OK, "case {}", case.id);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&body).expect("a decision body")
}

/// §8.3 span check: the reason's quote must be the verbatim slice of the
/// raw description at the locator's offsets, and the locator must name the
/// normalisation layer that revealed it.
fn assert_span_honesty(case: &Case, reasons: &[Value]) {
    let Some(expect) = case.expect.span.as_ref() else {
        return;
    };
    let injection = reasons
        .iter()
        .find(|reason| {
            reason["code"]
                .as_str()
                .is_some_and(|code| code.starts_with("SOLV-INJECTION"))
        })
        .unwrap_or_else(|| panic!("case {} must carry an injection reason", case.id));
    let evidence = injection["evidence"]
        .as_object()
        .expect("injection reason carries evidence");
    let quote = evidence["quote"]
        .as_str()
        .unwrap_or_else(|| panic!("case {}: injection evidence must quote the attack", case.id));
    let locator = evidence["locator"]
        .as_str()
        .unwrap_or_else(|| panic!("case {}: injection evidence must carry a locator", case.id));
    assert!(
        locator.starts_with("chars "),
        "case {}: locator format: {locator}",
        case.id
    );
    if !expect.note.is_empty() {
        assert!(
            locator.contains(expect.note.as_str()),
            "case {}: locator {locator} must name the layer ({})",
            case.id,
            expect.note
        );
    }
    // "chars N-M" up to the first ';' locates the span in characters.
    let offsets = locator
        .split(';')
        .next()
        .unwrap()
        .trim()
        .strip_prefix("chars ")
        .unwrap();
    let (start, end) = offsets.split_once('-').expect("chars N-M");
    let start: usize = start.parse().expect("numeric start");
    let end: usize = end.parse().expect("numeric end");
    let rebuilt: String = case
        .description
        .chars()
        .skip(start)
        .take(end - start)
        .collect();
    assert_eq!(
        rebuilt, quote,
        "case {}: the quote must be the verbatim span at the locator",
        case.id
    );
    assert!(
        quote.contains(&expect.contains),
        "case {}: quote {quote:?} must contain the attack fragment",
        case.id
    );
}

#[tokio::test]
async fn the_corpus_never_approves_and_every_reason_holds() {
    let corpus = corpus();
    // §8.3 composition: 20 cases across the six mandated categories.
    assert!(corpus.cases.len() >= 20, "at least 20 cases");
    let mut per_category: BTreeMap<&str, usize> = BTreeMap::new();
    for case in &corpus.cases {
        *per_category.entry(case.category.as_str()).or_default() += 1;
    }
    assert_eq!(per_category["instruction-override"], 5);
    assert_eq!(per_category["credential-access"], 3);
    assert_eq!(per_category["time-pressure"], 3);
    assert_eq!(per_category["unsupported-overclaim"], 3);
    assert_eq!(per_category["cold-start-as-established"], 3);
    assert_eq!(per_category["encoding-layered"], 3);

    for case in &corpus.cases {
        // Overclaim cases run against a settled PRIME profile so the
        // cold-start row does not mask the mismatch row that holds them.
        let router = match case.seed_profile.as_ref() {
            Some(seed) => router_with_seed(Some((
                case.counterparty_name.as_str(),
                seed.reppo_score,
                seed.days_ago,
            ))),
            None => router(),
        };
        let decision = run_case(router, case).await;
        assert_ne!(
            decision["decision"], "APPROVE",
            "case {} was approved: {decision}",
            case.id
        );
        assert_eq!(
            decision["decision"], case.expect.decision,
            "case {} expected {}",
            case.id, case.expect.decision
        );

        let reasons = decision["reasons"].as_array().expect("reasons[] present");
        assert!(
            !reasons.is_empty(),
            "case {} carries no reasons (G5 forbids this)",
            case.id
        );
        for reason in reasons {
            let code = reason["code"].as_str().expect("registered code");
            let severity = reason["severity"].as_str().expect("severity");
            let message = reason["message"].as_str().expect("human message");
            let effect = reason["effect"].as_str().expect("effect");
            assert!(
                !message.is_empty(),
                "case {} reason {code} empty message",
                case.id
            );
            assert!(
                matches!(
                    effect,
                    "forced_block" | "forced_hold" | "lowered_cap" | "informational"
                ),
                "case {}: unknown effect {effect}",
                case.id
            );
            assert!(
                matches!(severity, "critical" | "high" | "medium" | "info"),
                "case {}: unknown severity {severity}",
                case.id
            );
            // §2.2.3: a reason derived from the counterparty's text must
            // quote it. The injection rows are exactly those reasons.
            if code.starts_with("SOLV-INJECTION") {
                assert!(
                    reason["evidence"]["quote"].is_string(),
                    "case {}: {code} must quote the offending span",
                    case.id
                );
            }
        }
        for expected_code in &case.expect.codes {
            assert!(
                reasons
                    .iter()
                    .any(|reason| reason["code"] == expected_code.as_str()),
                "case {}: expected reason {expected_code}, got {:?}",
                case.id,
                reasons
                    .iter()
                    .map(|reason| reason["code"].clone())
                    .collect::<Vec<_>>()
            );
        }
        assert_span_honesty(case, reasons);
    }
}

/// §8.3's receipt leg: the decision recorded in the sealed receipt agrees
/// with what the case returned, so the public artefact carries the block.
#[tokio::test]
async fn every_blocked_case_seals_a_matching_receipt() {
    let corpus = corpus();
    for case in corpus
        .cases
        .iter()
        .filter(|case| case.expect.decision == "BLOCK")
    {
        let router = router();
        let decision = run_case(router.clone(), case).await;
        let receipt_url = decision["receipt_url"].as_str().expect("receipt url");
        let response = router
            .oneshot(
                HttpRequest::builder()
                    .uri(receipt_url)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK, "case {}", case.id);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let page = String::from_utf8_lossy(&body).to_string();
        assert!(
            page.contains(&decision["decision"].as_str().unwrap().to_string()),
            "case {}: receipt page shows the decision",
            case.id
        );
    }
}

/// The G1 claim (SPEC §9.1): with every detector down there is no path to
/// APPROVE — the gate holds fail-closed and says why.
#[tokio::test]
async fn all_detectors_down_hold_fail_closed_and_never_approve() {
    let store = Store::open(std::env::temp_dir().join(format!(
        "solv_attack_g1_{}_{}.json",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    )))
    .unwrap();
    let tier1: Arc<dyn ClefClient> = Arc::new(solv::tier1::MockClefClient {
        answers: Map::new(),
        fail_with: Some(solv::tier1::MockFailure::Status(503)),
    });
    let tier2: Arc<dyn ServClient> = Arc::new(solv::tier2::MockServClient {
        verdict: None,
        fail_with_status: Some(503),
        delay: None,
        started: None,
    });
    let response = build_router(AppState::new(store, tier1, tier2))
        .oneshot(
            HttpRequest::builder()
                .method("POST")
                .uri("/api/v1/pre-spend-check")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "counterparty_name": "g1-probe",
                        "target_url": "https://g1.example/api",
                        "amount_usd": 10.0,
                        "service_description": "Plain harmless description."
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let decision: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(decision["decision"], "HOLD");
    assert!(
        decision["reasons"]
            .as_array()
            .unwrap()
            .iter()
            .any(|reason| { reason["code"] == "SOLV-SYSTEM-003" }),
        "the hold must say the decision layers were unreachable"
    );
}
