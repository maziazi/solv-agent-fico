//! Reppo client tests (docs/SPEC.md §5.3) — all offline: the submit body
//! against the documented shape, status parsing (including every case where
//! a score must be refused rather than guessed), and the real client driven
//! against a localhost server. The one live test is `#[ignore]`d per
//! AGENTS.md; it spends Reppo's non-renewable quota and needs the project
//! owner's approval.

use std::time::Duration;

use axum::Json;
use axum::extract::Path;
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post};
use serde_json::{Value, json};
use solv::bureau::{AuditRequest, Bureau, BureauConfig, JobStatus};
use solv::reppo::{
    ALLOWED_SETTLEMENT_MINUTES, EvalStatus, EvalSubmission, MAX_PAYLOAD_BYTES, PILLAR_CRITERIA,
    ReppoClient, ReppoError, ReppoHttpClient, SubmissionError, parse_rfc3339_unix,
    parse_status_response, sanitize_untrusted,
};
use solv::score::Pillars;
use solv::state::Store;

fn submission() -> EvalSubmission {
    EvalSubmission::new(
        r#"{"counterparty_name":"DataFeed Pro"}"#.to_string(),
        Some("fixed context".to_string()),
        10,
    )
    .unwrap()
}

#[test]
fn submit_body_matches_the_documented_shape() {
    let body = submission().to_body();
    assert_eq!(body["type"], "artifact");
    assert_eq!(body["settlementMinutes"], 10);
    assert_eq!(body["context"], "fixed context");
    assert_eq!(body["payload"], r#"{"counterparty_name":"DataFeed Pro"}"#);

    // SPEC §4.1: the four pillar questions, in the order that maps the
    // returned scores back onto reliability/track record/safety/maturity.
    let criteria = body["criteria"].as_array().unwrap();
    assert_eq!(criteria.len(), 4);
    for (sent, expected) in criteria.iter().zip(PILLAR_CRITERIA) {
        assert_eq!(sent, expected);
        assert!(expected.len() <= 2 * 1024, "criteria are at most 2 KB each");
    }
    assert!(criteria[2].as_str().unwrap().contains("buying agent"));
}

#[test]
fn submission_limits_are_enforced_locally() {
    assert_eq!(
        EvalSubmission::new(String::new(), None, 10),
        Err(SubmissionError::EmptyPayload)
    );
    assert_eq!(
        EvalSubmission::new("x".repeat(MAX_PAYLOAD_BYTES + 1), None, 10),
        Err(SubmissionError::PayloadTooLarge(MAX_PAYLOAD_BYTES + 1))
    );
    assert!(EvalSubmission::new("x".repeat(MAX_PAYLOAD_BYTES), None, 10).is_ok());
    assert!(matches!(
        EvalSubmission::new("p".to_string(), Some("c".repeat(32 * 1024 + 1)), 10),
        Err(SubmissionError::ContextTooLarge(_))
    ));
    // SPEC §5.3: the minimum settlement window is 10 minutes.
    for minutes in [0, 1, 5, 9, 11, 45, 1441] {
        assert_eq!(
            EvalSubmission::new("p".to_string(), None, minutes),
            Err(SubmissionError::BadSettlementMinutes(minutes))
        );
    }
    for minutes in ALLOWED_SETTLEMENT_MINUTES {
        assert!(EvalSubmission::new("p".to_string(), None, minutes).is_ok());
    }
}

fn settled_body(score: Value, verdict_scores: [f64; 4]) -> Value {
    let verdicts: Vec<Value> = PILLAR_CRITERIA
        .iter()
        .zip(verdict_scores)
        .map(|(criterion, score)| {
            json!({ "criterion": criterion, "score": score, "critique": "because" })
        })
        .collect();
    json!({
        "evalId": "ev_1",
        "status": "SETTLED",
        "creditsCharged": 100,
        "settledAt": "2026-10-14T17:18:42.000Z",
        "result": {
            "verdict": { "score": score, "scale": "0-100", "decision": "revise", "confidence": "high" },
            "verdicts": verdicts,
            "action": { "route": "review" },
            "evidence": { "evidenceBasis": "model-judgment", "pods": [], "peerAssessmentCount": 2 }
        }
    })
}

#[test]
fn a_settled_evaluation_parses_into_a_validated_score() {
    let status = parse_status_response(&settled_body(json!(78), [7.0, 8.0, 9.0, 6.0])).unwrap();
    let EvalStatus::Settled(settled) = status else {
        panic!("expected Settled, got {status:?}");
    };
    assert_eq!(settled.reppo_score, 78.0);
    assert_eq!(settled.settled_at_unix, Some(1_791_998_322));
    assert_eq!(settled.credits_charged, 100);
    assert_eq!(settled.route.as_deref(), Some("review"));
    assert_eq!(
        settled.pillars,
        Pillars {
            reliability: 7,
            track_record: 8,
            safety: 9,
            maturity: 6,
        }
    );
    assert_eq!(settled.verdicts.len(), 4);
    assert_eq!(settled.verdicts[2].critique, "because");
}

#[test]
fn pillars_follow_criterion_text_not_array_position() {
    let mut body = settled_body(json!(60), [1.0, 2.0, 3.0, 4.0]);
    body["result"]["verdicts"].as_array_mut().unwrap().reverse();
    let EvalStatus::Settled(settled) = parse_status_response(&body).unwrap() else {
        panic!("expected Settled");
    };
    assert_eq!(settled.pillars.reliability, 1);
    assert_eq!(
        settled.pillars.safety, 3,
        "safety is whichever carries its question"
    );
    assert_eq!(settled.pillars.maturity, 4);
}

#[test]
fn reworded_criteria_fall_back_to_order_only_when_there_are_exactly_four() {
    let mut body = settled_body(json!(60), [5.0, 6.0, 2.0, 8.0]);
    for entry in body["result"]["verdicts"].as_array_mut().unwrap() {
        entry["criterion"] = json!("reworded by reppo");
    }
    let EvalStatus::Settled(settled) = parse_status_response(&body).unwrap() else {
        panic!("expected Settled");
    };
    assert_eq!(settled.pillars.safety, 2);

    // Three verdicts cannot be attributed to four pillars: refuse.
    body["result"]["verdicts"].as_array_mut().unwrap().pop();
    assert!(matches!(
        parse_status_response(&body),
        Err(ReppoError::Parse(_))
    ));
}

#[test]
fn fractional_pillar_scores_floor_toward_the_safe_side() {
    // A safety score of 3.5 must not become 4 and slip past the <= 3
    // hard gate (SPEC §5.4).
    let EvalStatus::Settled(settled) =
        parse_status_response(&settled_body(json!(50), [7.9, 8.0, 3.5, 6.0])).unwrap()
    else {
        panic!("expected Settled");
    };
    assert_eq!(settled.pillars.safety, 3);
    assert_eq!(settled.pillars.reliability, 7);
}

#[test]
fn out_of_range_or_missing_scores_are_refused_not_clamped() {
    for bad in [json!(100.5), json!(-1), json!("78"), Value::Null] {
        assert!(
            matches!(
                parse_status_response(&settled_body(bad.clone(), [7.0, 8.0, 9.0, 6.0])),
                Err(ReppoError::Parse(_))
            ),
            "verdict.score {bad} must be refused"
        );
    }
    for bad in [10.5, -0.5, f64::INFINITY] {
        // serde_json cannot carry NaN/inf, so a non-finite score arrives as null.
        let body = settled_body(json!(50), [bad, 8.0, 9.0, 6.0]);
        assert!(
            matches!(parse_status_response(&body), Err(ReppoError::Parse(_))),
            "pillar score {bad} must be refused"
        );
    }
    let mut no_result = settled_body(json!(50), [7.0, 8.0, 9.0, 6.0]);
    no_result["result"] = Value::Null;
    assert!(matches!(
        parse_status_response(&no_result),
        Err(ReppoError::Parse(_))
    ));
}

#[test]
fn the_other_statuses_parse() {
    assert_eq!(
        parse_status_response(&json!({ "status": "PENDING", "result": null })).unwrap(),
        EvalStatus::Pending
    );
    assert_eq!(
        parse_status_response(&json!({ "status": "FAILED", "result": { "reason": "no quorum" } }))
            .unwrap(),
        EvalStatus::Failed(Some("no quorum".to_string()))
    );
    assert_eq!(
        parse_status_response(&json!({ "status": "FAILED", "result": null })).unwrap(),
        EvalStatus::Failed(None)
    );
    assert_eq!(
        parse_status_response(&json!({ "status": "DENIED" })).unwrap(),
        EvalStatus::Denied
    );
    assert!(matches!(
        parse_status_response(&json!({ "status": "MAYBE" })),
        Err(ReppoError::Parse(_))
    ));
    assert!(matches!(
        parse_status_response(&json!({})),
        Err(ReppoError::Parse(_))
    ));
}

async fn reppo_server(app: axum::Router) -> (String, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (format!("http://{addr}"), task)
}

fn client(base: String) -> ReppoHttpClient {
    ReppoHttpClient::with_base_url("test-key".to_string(), Duration::from_secs(2), base).unwrap()
}

fn authorised(headers: &HeaderMap) -> bool {
    headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        == Some("Bearer test-key")
}

#[tokio::test]
async fn real_client_submits_and_reads_over_http() {
    let app = axum::Router::new()
        .route(
            "/agents/eval-requests/submit",
            post(|headers: HeaderMap, Json(body): Json<Value>| async move {
                if !authorised(&headers) {
                    return (StatusCode::UNAUTHORIZED, Json(json!({})));
                }
                assert_eq!(body["type"], "artifact");
                assert_eq!(body["criteria"].as_array().unwrap().len(), 4);
                (
                    StatusCode::CREATED,
                    Json(json!({ "id": "internal", "evalId": "ev_77", "status": "PENDING" })),
                )
            }),
        )
        .route(
            "/agents/eval-requests/{eval_id}",
            get(
                |headers: HeaderMap, Path(eval_id): Path<String>| async move {
                    if !authorised(&headers) {
                        return (StatusCode::UNAUTHORIZED, Json(json!({})));
                    }
                    assert_eq!(eval_id, "ev_77");
                    (
                        StatusCode::OK,
                        Json(settled_body(json!(78), [7.0, 8.0, 9.0, 6.0])),
                    )
                },
            ),
        );
    let (base, _server) = reppo_server(app).await;
    let reppo = client(base);

    let eval_id = reppo.submit(&submission()).await.unwrap();
    assert_eq!(
        eval_id, "ev_77",
        "the evalId is used, never the internal id"
    );
    let status = reppo.status(&eval_id).await.unwrap();
    assert!(matches!(status, EvalStatus::Settled(_)));
}

#[tokio::test]
async fn real_client_maps_http_statuses_to_distinct_errors() {
    for code in [401u16, 402, 409, 500] {
        let app = axum::Router::new().route(
            "/agents/eval-requests/submit",
            post(move || async move { StatusCode::from_u16(code).unwrap() }),
        );
        let (base, _server) = reppo_server(app).await;
        let error = client(base).submit(&submission()).await.unwrap_err();
        let matched = match code {
            401 => matches!(error, ReppoError::Unauthorized),
            402 => matches!(error, ReppoError::InsufficientCredits),
            409 => matches!(error, ReppoError::PendingConflict),
            _ => matches!(error, ReppoError::Status(500)),
        };
        assert!(matched, "HTTP {code} mapped to {error:?}");
    }

    let app = axum::Router::new().route(
        "/agents/eval-requests/{eval_id}",
        get(|| async { StatusCode::NOT_FOUND }),
    );
    let (base, _server) = reppo_server(app).await;
    assert!(matches!(
        client(base).status("ev_other").await,
        Err(ReppoError::NotFound)
    ));
}

#[tokio::test]
async fn real_client_rejects_a_submit_response_without_an_eval_id() {
    let app = axum::Router::new().route(
        "/agents/eval-requests/submit",
        post(|| async { (StatusCode::CREATED, Json(json!({ "id": "internal-only" }))) }),
    );
    let (base, _server) = reppo_server(app).await;
    assert!(matches!(
        client(base).submit(&submission()).await,
        Err(ReppoError::Parse(_))
    ));
}

#[test]
fn rfc3339_overflowing_input_is_none_and_never_panics() {
    // Each of these used to overflow i64 arithmetic: a panic in a debug
    // build, silent wraparound in release.
    for hostile in [
        "9223372036854775807-10-14T17:18:42Z",
        "2026-10-14T17:18:42+9223372036854775807:00",
        "2026-10-14T17:18:42-9223372036854775807:00",
        "2026-10-14T9223372036854775807:18:42Z",
        "2026-10-14T17:18:9223372036854775807Z",
        "2026-9223372036854775807-14T17:18:42Z",
        "2026-10-9223372036854775807T17:18:42Z",
        "99999999999999999999-10-14T17:18:42Z",
    ] {
        assert_eq!(parse_rfc3339_unix(hostile), None, "{hostile}");
    }
}

#[test]
fn rfc3339_is_bounded_to_years_1970_through_9999_and_sane_offsets() {
    assert_eq!(parse_rfc3339_unix("1969-12-31T23:59:59Z"), None);
    assert!(parse_rfc3339_unix("1970-01-01T00:00:00Z").is_some());
    assert!(parse_rfc3339_unix("9999-12-31T23:59:59Z").is_some());
    assert_eq!(parse_rfc3339_unix("10000-01-01T00:00:00Z"), None);

    // |offset| <= 23:59.
    assert!(parse_rfc3339_unix("2026-10-14T17:18:42+23:59").is_some());
    assert!(parse_rfc3339_unix("2026-10-14T17:18:42-23:59").is_some());
    for bad in ["+24:00", "-24:00", "+00:60", "+99:99", "+1:00", "+01:0"] {
        assert_eq!(
            parse_rfc3339_unix(&format!("2026-10-14T17:18:42{bad}")),
            None,
            "offset {bad}"
        );
    }
}

#[test]
fn rfc3339_rejects_malformed_shapes_instead_of_guessing() {
    for bad in [
        "2026-10-14T17:18:42",
        "2026-10-14T17:18:42Z+01:00",
        "2026-10-14T17:18:42Zjunk",
        "2026-02-30T00:00:00Z",
        "2026-10-14T24:00:00Z",
        "2026-10-14T17:60:00Z",
        "2026-10-14T17:18:61Z",
        "+026-10-14T17:18:42Z",
        "2026-10-14T+7:18:42Z",
        "2026-10-14T17:18:42.Z",
        "2026-10-14 17:18:42Z",
        "",
    ] {
        assert_eq!(parse_rfc3339_unix(bad), None, "{bad:?}");
    }
    // Fractional seconds are dropped, not rounded.
    assert_eq!(
        parse_rfc3339_unix("2026-10-14T17:18:42.999Z"),
        parse_rfc3339_unix("2026-10-14T17:18:42Z")
    );
}

#[test]
fn untrusted_text_loses_control_characters_and_is_bounded() {
    assert_eq!(
        sanitize_untrusted("line one\nline\ttwo\r\u{1b}[31mred\u{7}", 200),
        "line onelinetwo[31mred"
    );
    // Bidi overrides and zero-width characters are invisible tricks.
    assert_eq!(
        sanitize_untrusted("a\u{202e}b\u{200b}c\u{2066}d", 200),
        "abcd"
    );
    // Exactly at the limit is untouched; one over is cut and marked.
    let at_limit = "x".repeat(200);
    assert_eq!(sanitize_untrusted(&at_limit, 200), at_limit);
    let over = sanitize_untrusted(&"y".repeat(5_000), 200);
    assert_eq!(over.chars().count(), 200);
    assert!(over.ends_with("..."));
    // Multi-byte text is cut on a character boundary, never mid-byte.
    let cut = sanitize_untrusted(&"é".repeat(1_000), 50);
    assert_eq!(cut.chars().count(), 50);
    assert_eq!(sanitize_untrusted("", 200), "");
}

#[tokio::test]
async fn an_unreadable_submit_body_is_logged_sanitized_so_the_eval_id_can_be_recovered() {
    // Control characters first, then a recoverable id buried in a body that
    // is not the documented shape, then a long tail.
    let noisy = format!(
        "\u{7}\u{1b}[2J{{\"data\": {{\"evalIdTypo\": \"ev_recover_me\"}}}}{}",
        "z".repeat(5_000)
    );
    let app = axum::Router::new().route(
        "/agents/eval-requests/submit",
        post(move || {
            let body = noisy.clone();
            async move { (StatusCode::CREATED, body) }
        }),
    );
    let (base, _server) = reppo_server(app).await;
    let error = client(base).submit(&submission()).await.unwrap_err();
    let ReppoError::Parse(message) = error else {
        panic!("expected Parse, got {error:?}");
    };
    assert!(message.contains("ev_recover_me"), "{message}");
    assert!(
        !message.chars().any(char::is_control),
        "no control characters survive: {message:?}"
    );
    assert!(message.chars().count() < 400, "bounded: {}", message.len());

    // Valid JSON that lacks `evalId` gets the same treatment.
    let app = axum::Router::new().route(
        "/agents/eval-requests/submit",
        post(|| async {
            (
                StatusCode::CREATED,
                Json(json!({ "id": "internal-only", "note": "ev_in_other_field" })),
            )
        }),
    );
    let (base, _server) = reppo_server(app).await;
    let ReppoError::Parse(message) = client(base).submit(&submission()).await.unwrap_err() else {
        panic!("expected Parse");
    };
    assert!(message.contains("evalId missing"), "{message}");
    assert!(message.contains("ev_in_other_field"), "{message}");
}

/// Hits the real Reppo Evaluation API through the whole worker path:
/// enqueue, submit, wait out the 10-minute window, poll to `SETTLED`, and
/// confirm the profile gained a score. Never runs in `cargo test`.
///
/// **This spends 100 of the 1,000 non-renewable free credits and takes at
/// least ten minutes.** Run it only with the project owner's approval, with
/// `REPPO_API_KEY` in a local `.env`:
///
/// ```sh
/// cargo test --test reppo live_reppo -- --ignored --nocapture
/// ```
#[tokio::test]
#[ignore = "spends 100 real Reppo credits (non-renewable) and takes 10+ minutes; needs REPPO_API_KEY and owner approval"]
async fn live_reppo_evaluation_settles_and_updates_a_profile() {
    dotenvy::dotenv().ok();
    let reppo = ReppoHttpClient::from_env().expect("REPPO_API_KEY must be set");
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("state.json")).unwrap();
    let (bureau, _worker) = Bureau::spawn(
        std::sync::Arc::new(reppo),
        std::sync::Arc::clone(&store),
        BureauConfig::default(),
    );

    let enqueued = bureau
        .enqueue(AuditRequest {
            counterparty_id: "solv-live-test".to_string(),
            counterparty_name: "Solv live test".to_string(),
            target_url: "https://eval.reppo.xyz".to_string(),
            claim: "Evaluation API for AI agent outputs, scored by staked evaluators.".to_string(),
        })
        .expect("enqueue");

    // Settlement takes at least ten minutes, usually a few more.
    let deadline = std::time::Instant::now() + Duration::from_secs(60 * 60);
    let status = loop {
        let status = bureau.job_status(&enqueued.job_id).expect("job known");
        if status.is_terminal() {
            break status;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "evaluation did not settle within an hour; last status {status:?}"
        );
        tokio::time::sleep(Duration::from_secs(15)).await;
    };

    assert!(
        matches!(status, JobStatus::Settled { .. }),
        "live evaluation did not settle: {status:?}"
    );
    let profile = store.profile("solv-live-test").expect("profile written");
    let score = profile.score.expect("profile carries a settled score");
    assert!((0.0..=100.0).contains(&score.reppo_score));
    assert!(!score.eval_id.is_empty());
}
