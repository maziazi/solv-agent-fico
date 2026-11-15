//! Tier 1 client tests (docs/SPEC.md §5.1) — all offline: the request
//! shape against the SPEC example, answer parsing into gate signals, the
//! mock, and the real client exercised against a localhost server
//! (success, error status, timeout). The one live-endpoint test is
//! `#[ignore]`d per AGENTS.md.

use std::time::Duration;

use axum::Json;
use axum::routing::post;
use serde_json::json;
use solv::policy::{RiskClass, Tier1Signals};
use solv::tier1::{
    ClefClient, ClefError, ClefState, CloudflareClefClient, MockClefClient, standard_request,
};

fn state() -> ClefState {
    ClefState {
        counterparty_name: "DataFeed Pro".to_string(),
        target_url: "https://api.example.com/v1/feed".to_string(),
        amount_usd: 50.0,
        service_description: "Real-time market data, 1 month access.".to_string(),
        known_profile: None,
    }
}

#[tokio::test]
async fn standard_request_matches_the_spec_example_shape() {
    let body = standard_request(state()).to_body();
    assert_eq!(body["model"], "clef-flash");

    // State is data: every field is a value, none is an instruction.
    assert_eq!(body["state"]["counterparty_name"], "DataFeed Pro");
    assert_eq!(
        body["state"]["target_url"],
        "https://api.example.com/v1/feed"
    );
    assert_eq!(body["state"]["amount_usd"], 50.0);
    assert_eq!(
        body["state"]["service_description"],
        "Real-time market data, 1 month access."
    );
    assert!(body["state"].get("known_profile").is_none());

    let questions = body["questions"].as_object().unwrap();
    assert_eq!(questions.len(), 4, "the four fixed questions");

    assert_eq!(questions["injection"]["type"], "noul");
    assert!(questions["injection"]["criteria"]["true"].is_string());
    assert_eq!(questions["claim_plausibility"]["type"], "score");
    assert_eq!(
        questions["claim_plausibility"]["criteria"]
            .as_array()
            .map(Vec::len),
        Some(4),
        "four ordered levels"
    );
    assert_eq!(questions["risk_class"]["type"], "choice");
    assert_eq!(
        questions["risk_class"]["criteria"]
            .as_object()
            .map(|m| m.len()),
        Some(5),
        "five risk classes"
    );
    assert_eq!(questions["profile_consistency"]["type"], "noul");
    assert!(
        questions["profile_consistency"].get("criteria").is_none(),
        "criteria is optional for noul"
    );
}

#[tokio::test]
async fn known_profile_serialises_into_state() {
    let mut with_profile = state();
    with_profile.known_profile = Some(solv::tier1::KnownProfile {
        fico_effective: 729,
        tier: "PRIME".to_string(),
        age_days: 12,
        pillars: solv::score::Pillars {
            reliability: 8,
            track_record: 7,
            safety: 9,
            maturity: 7,
        },
    });
    let body = standard_request(with_profile).to_body();
    assert_eq!(body["state"]["known_profile"]["fico_effective"], 729);
    assert_eq!(body["state"]["known_profile"]["tier"], "PRIME");
}

#[tokio::test]
async fn mock_answers_parse_into_gate_signals() {
    let client = MockClefClient::with_injection(0.03);
    let response = client.ask(&standard_request(state())).await.unwrap();
    let signals = response.to_signals();
    assert_eq!(
        signals,
        Tier1Signals {
            available: true,
            injection_probability_true: Some(0.03),
            risk_class: Some(RiskClass::None),
            risk_class_confidence: Some(0.97),
        }
    );
    assert!(signals.has_verdict());
}

#[tokio::test]
async fn answerless_response_is_not_a_verdict() {
    let client = MockClefClient::default();
    let response = client.ask(&standard_request(state())).await.unwrap();
    let signals = response.to_signals();
    assert!(!signals.available);
    assert!(!signals.has_verdict());
}

#[tokio::test]
async fn unknown_risk_class_choice_is_dropped() {
    let mut answers = serde_json::Map::new();
    answers.insert(
        "risk_class".to_string(),
        json!({ "choice": "banana", "confidence": 0.99 }),
    );
    answers.insert("injection".to_string(), json!({ "probability_true": 0.05 }));
    let client = MockClefClient {
        answers,
        fail_with: None,
    };
    let signals = client
        .ask(&standard_request(state()))
        .await
        .unwrap()
        .to_signals();
    assert_eq!(signals.risk_class, None);
    assert_eq!(signals.injection_probability_true, Some(0.05));
}

/// Drives the real client offline: a localhost axum server standing in for
/// Cloudflare with a canned Workers-AI-shaped response. The handler also
/// captures what the client actually sent: bearer auth and body shape are
/// verified through the real HTTP path, not just `to_body()`.
#[tokio::test]
async fn real_client_parses_a_workers_ai_response() {
    let (sent_tx, mut sent_rx) = tokio::sync::mpsc::unbounded_channel();
    let sent = std::sync::Arc::new(sent_tx);
    let app = axum::Router::new().route(
        "/accounts/{account}/ai/run/@cf/cloudflare/clef-flash",
        post(move |headers: axum::http::HeaderMap, body: String| {
            let sent = std::sync::Arc::clone(&sent);
            async move {
                let _ = sent.send((headers, body));
                Json(json!({
                    "success": true,
                    "errors": [],
                    "messages": [],
                    "result": {
                        "answers": {
                            "injection": { "probability_true": 0.91 },
                            "risk_class": { "choice": "exfiltration", "confidence": 0.94 }
                        }
                    }
                }))
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let client = CloudflareClefClient::with_base_url(
        "test-account".to_string(),
        "test-token".to_string(),
        Duration::from_secs(2),
        format!("http://{}", addr),
    )
    .unwrap();
    let response = client.ask(&standard_request(state())).await.unwrap();
    let (headers, body) = sent_rx.recv().await.unwrap();
    assert_eq!(
        headers.get("authorization").and_then(|v| v.to_str().ok()),
        Some("Bearer test-token")
    );
    let body: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(body["model"], "clef-flash");
    assert!(body["questions"]["injection"]["type"].is_string());

    let signals = response.to_signals();
    assert_eq!(signals.injection_probability_true, Some(0.91));
    assert_eq!(signals.risk_class, Some(RiskClass::Exfiltration));
    assert_eq!(signals.risk_class_confidence, Some(0.94));
}

#[tokio::test]
async fn real_client_maps_error_status_and_bad_shape() {
    let app = axum::Router::new().route(
        "/accounts/{account}/ai/run/@cf/cloudflare/clef-flash",
        post(|| async {
            // First consumer of this test gets a 503, then a 200 with a
            // body missing result.answers: axum cannot easily vary per
            // call, so two servers on two ports do it.
            (axum::http::StatusCode::SERVICE_UNAVAILABLE, "down")
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let client = CloudflareClefClient::with_base_url(
        "test-account".to_string(),
        "test-token".to_string(),
        Duration::from_secs(2),
        format!("http://{}", addr),
    )
    .unwrap();
    match client.ask(&standard_request(state())).await {
        Err(ClefError::Status(503)) => {}
        other => panic!("expected Status(503), got {other:?}"),
    }

    // A 200 without result.answers is a parse error, not success.
    let app2 = axum::Router::new().route(
        "/accounts/{account}/ai/run/@cf/cloudflare/clef-flash",
        post(|| async { Json(json!({ "success": true, "result": {} })) }),
    );
    let listener2 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr2 = listener2.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener2, app2).await.unwrap() });
    // Re-point the same client at the second server.
    let client = CloudflareClefClient::with_base_url(
        "test-account".to_string(),
        "test-token".to_string(),
        Duration::from_secs(2),
        format!("http://{}", addr2),
    )
    .unwrap();
    match client.ask(&standard_request(state())).await {
        Err(ClefError::Parse(_)) => {}
        other => panic!("expected Parse error, got {other:?}"),
    }
}

#[tokio::test]
async fn real_client_times_out_rather_than_hanging() {
    let app = axum::Router::new().route(
        "/accounts/{account}/ai/run/@cf/cloudflare/clef-flash",
        post(|| async {
            tokio::time::sleep(Duration::from_millis(500)).await;
            Json(json!({ "result": { "answers": {} } }))
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let client = CloudflareClefClient::with_base_url(
        "test-account".to_string(),
        "test-token".to_string(),
        Duration::from_millis(50),
        format!("http://{}", addr),
    )
    .unwrap();
    let started = std::time::Instant::now();
    match client.ask(&standard_request(state())).await {
        Err(ClefError::Http(error)) => {
            assert!(error.is_timeout(), "expected a timeout error, got: {error}")
        }
        other => panic!("expected Http timeout, got {other:?}"),
    }
    assert!(started.elapsed() < Duration::from_millis(400));
}

/// Hits the real Cloudflare endpoint. Never runs in `cargo test`; run
/// explicitly with credentials in a local `.env` (AGENTS.md forbids paid
/// credits in the automated test path):
///
/// ```sh
/// cargo test --test tier1 live_clef_flash -- --ignored --nocapture
/// ```
#[tokio::test]
#[ignore = "spends one real Clef-flash call; needs CLOUDFLARE_ACCOUNT_ID and CLOUDFLARE_API_TOKEN"]
async fn live_clef_flash_call_answers_the_standard_questions() {
    // Test binaries never run main(), so the .env must be loaded here.
    dotenvy::dotenv().ok();
    let client = CloudflareClefClient::from_env()
        .expect("CLOUDFLARE_ACCOUNT_ID and CLOUDFLARE_API_TOKEN must be set");
    let response = client
        .ask(&standard_request(state()))
        .await
        .expect("live Clef-flash call failed");
    let injection = response
        .injection_probability_true()
        .expect("live answers must include injection.probability_true");
    assert!(
        (0.0..=1.0).contains(&injection),
        "probability out of range: {injection}"
    );
}

#[tokio::test]
async fn mock_failure_shapes_are_returned_verbatim() {
    let status_mock = MockClefClient {
        fail_with: Some(solv::tier1::MockFailure::Status(429)),
        ..MockClefClient::default()
    };
    match status_mock.ask(&standard_request(state())).await {
        Err(ClefError::Status(429)) => {}
        other => panic!("expected Status(429), got {other:?}"),
    }
    let parse_mock = MockClefClient {
        fail_with: Some(solv::tier1::MockFailure::Parse),
        ..MockClefClient::default()
    };
    match parse_mock.ask(&standard_request(state())).await {
        Err(ClefError::Parse(_)) => {}
        other => panic!("expected Parse, got {other:?}"),
    }
}

#[test]
fn half_specified_noul_criteria_serialize_instead_of_dropping() {
    let question = solv::tier1::ClefQuestion {
        key: "learned_1".to_string(),
        instructions: "Question text.".to_string(),
        kind: solv::tier1::QuestionKind::Noul {
            true_text: Some("Yes.".to_string()),
            false_text: None,
        },
    };
    let value = question.to_value();
    assert_eq!(value["criteria"]["true"], "Yes.");
    assert!(value["criteria"].get("false").is_none());
}
