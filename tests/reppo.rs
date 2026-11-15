//! Reppo Eval Router client tests (docs/SPEC.md §5.3, migrated 2026-11-15 to
//! the Eval Router API) — all offline: the submit body against the
//! documented shape, status parsing (including every case where a score
//! must be refused rather than guessed), and the real client driven against
//! a localhost server. The one live test is `#[ignore]`d per AGENTS.md; it
//! spends Reppo credits and needs the project owner's approval.

use std::time::Duration;

use axum::Json;
use axum::extract::Path;
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post};
use serde_json::{Value, json};
use solv::reppo::{
    CREDITS_PER_EVAL, EvalStatus, EvalSubmission, MAX_BODY_BYTES, MAX_INPUT_BYTES, ReppoClient,
    ReppoError, ReppoHttpClient, SubmissionError, parse_rfc3339_unix, parse_status_response,
    sanitize_untrusted,
};

fn submission() -> EvalSubmission {
    EvalSubmission::new(
        "Claim to fact-check, submitted by Solv.\n\nClaim: \"real-time market data, SLA 99.9%.\""
            .to_string(),
    )
    .unwrap()
}

#[test]
fn submit_body_matches_the_documented_shape() {
    let body = submission().to_body();
    assert_eq!(body["taskType"], "claim");
    assert_eq!(
        body["input"],
        "Claim to fact-check, submitted by Solv.\n\nClaim: \"real-time market data, SLA 99.9%.\""
    );

    // The Eval Router body carries nothing else: no criteria, no settlement
    // window, no context field (the context travels inside the input text).
    assert_eq!(body.as_object().unwrap().len(), 2);
}

#[test]
fn submission_limits_are_enforced_locally() {
    assert_eq!(
        EvalSubmission::new(String::new()),
        Err(SubmissionError::EmptyInput)
    );
    assert_eq!(
        EvalSubmission::new("x".repeat(MAX_INPUT_BYTES + 1)),
        Err(SubmissionError::InputTooLarge(MAX_INPUT_BYTES + 1))
    );
    assert!(EvalSubmission::new("x".repeat(MAX_INPUT_BYTES)).is_ok());
}

/// A COMPLETED evaluation in the Eval Router's shape. `rubric` is the
/// scorecard Reppo picked; every value may be 0–10 or 0–100 (both are
/// accepted and normalised, per the migrating design note).
fn completed_body(score: Value, rubric: Value) -> Value {
    json!({
        "id": "ev_1",
        "taskType": "claim",
        "status": "COMPLETED",
        "score": score,
        "route": "review",
        "creditsCharged": CREDITS_PER_EVAL,
        "completedAt": "2026-10-14T17:18:42.000Z",
        "result": {
            "score": score,
            "decision": "reject",
            "confidence": "high",
            "route": "review",
            "reason": "The claim is not supported by verifiable evidence.",
            "rubric_scores": rubric,
        }
    })
}

#[test]
fn a_completed_evaluation_parses_into_a_validated_score() {
    let status = parse_status_response(&completed_body(
        json!(78),
        json!({
            "reliability": 7, "track_record": 8, "safety": 9, "maturity": 6
        }),
    ))
    .unwrap();
    let EvalStatus::Settled(settled) = status else {
        panic!("expected Settled, got {status:?}");
    };
    assert_eq!(settled.reppo_score, 78.0);
    assert_eq!(settled.settled_at_unix, Some(1_791_998_322));
    assert_eq!(settled.credits_charged, CREDITS_PER_EVAL);
    assert_eq!(settled.route.as_deref(), Some("review"));
    // Keyword mapping: the dimensions Reppo chose cover all four pillars.
    assert_eq!(
        settled.pillars,
        solv::score::Pillars {
            reliability: 7,
            track_record: 8,
            safety: 9,
            maturity: 6
        }
    );
    // Distillation input: one verdict entry per scored dimension (the
    // object order is the map's, so assert the set of criteria).
    assert_eq!(settled.verdicts.len(), 4);
    let mut criteria: Vec<&str> = settled
        .verdicts
        .iter()
        .map(|verdict| verdict.criterion.as_str())
        .collect();
    criteria.sort_unstable();
    assert_eq!(
        criteria,
        ["maturity", "reliability", "safety", "track_record"]
    );
}

#[test]
fn rubric_values_on_a_hundred_point_scale_are_normalised_down() {
    let status = parse_status_response(&completed_body(
        json!(64),
        json!({
            "reliability": 70, "track_record": 85, "safety": 30, "maturity": 60
        }),
    ))
    .unwrap();
    let EvalStatus::Settled(settled) = status else {
        panic!("expected Settled");
    };
    // Floored toward the safe side: the Safety gate must not be passed by
    // rounding.
    assert_eq!(
        settled.pillars,
        solv::score::Pillars {
            reliability: 7,
            track_record: 8,
            safety: 3,
            maturity: 6
        }
    );
}

#[test]
fn uncovered_pillars_default_to_the_middle_of_the_scale() {
    // The Eval Router picks its own dimensions; a submission whose
    // scorecard covers none of our keywords gets the neutral default —
    // never a zero that reads as an accusation or a ten that reads as an
    // endorsement.
    let status = parse_status_response(&completed_body(
        json!(55),
        json!({
            "clarity": 8, "format": 9
        }),
    ))
    .unwrap();
    let EvalStatus::Settled(settled) = status else {
        panic!("expected Settled");
    };
    assert_eq!(
        settled.pillars,
        solv::score::Pillars {
            reliability: 5,
            track_record: 5,
            safety: 5,
            maturity: 5
        }
    );
}

#[test]
fn out_of_range_or_missing_scores_are_refused_not_clamped() {
    // A COMPLETED evaluation whose headline score is out of range is
    // refused: a clamped number would sit on a public receipt.
    assert!(matches!(
        parse_status_response(&completed_body(json!(101), json!({}))),
        Err(ReppoError::Parse(_))
    ));
    assert!(matches!(
        parse_status_response(&completed_body(json!(-1), json!({}))),
        Err(ReppoError::Parse(_))
    ));
    // A scorecard value outside both scales simply contributes nothing.
    let status = parse_status_response(&completed_body(
        json!(60),
        json!({
            "clarity": 8, "nonsense": 250
        }),
    ))
    .unwrap();
    let EvalStatus::Settled(settled) = status else {
        panic!("expected Settled");
    };
    assert_eq!(settled.pillars.reliability, 5);
    assert_eq!(settled.verdicts.len(), 1, "the nonsense value is dropped");
}

#[test]
fn the_other_statuses_parse() {
    assert_eq!(
        parse_status_response(&json!({ "status": "QUEUED", "result": null })).unwrap(),
        EvalStatus::Pending
    );
    assert_eq!(
        parse_status_response(&json!({ "status": "PROCESSING" })).unwrap(),
        EvalStatus::Pending
    );
    assert_eq!(
        parse_status_response(&json!({ "status": "FAILED", "error": "could not queue" })).unwrap(),
        EvalStatus::Failed(Some("could not queue".to_string()))
    );
    assert_eq!(
        parse_status_response(&json!({ "status": "FAILED", "error": null })).unwrap(),
        EvalStatus::Failed(None)
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
            "/agents/evals",
            post(|headers: HeaderMap, Json(body): Json<Value>| async move {
                if !authorised(&headers) {
                    return (StatusCode::UNAUTHORIZED, Json(json!({})));
                }
                assert_eq!(body["taskType"], "claim");
                assert!(body["input"].as_str().unwrap().contains("Claim"));
                assert!(body.as_object().unwrap().len() <= 3);
                (
                    StatusCode::CREATED,
                    Json(json!({
                        "id": "ev_77",
                        "taskType": "claim",
                        "status": "QUEUED",
                        "creditsCharged": 0
                    })),
                )
            }),
        )
        .route(
            "/agents/evals/{eval_id}",
            get(
                |headers: HeaderMap, Path(eval_id): Path<String>| async move {
                    if !authorised(&headers) {
                        return (StatusCode::UNAUTHORIZED, Json(json!({})));
                    }
                    assert_eq!(eval_id, "ev_77");
                    (
                        StatusCode::OK,
                        Json(json!({
                            "id": "ev_77",
                            "status": "COMPLETED",
                            "score": 64,
                            "route": "review",
                            "creditsCharged": CREDITS_PER_EVAL,
                            "result": { "score": 64, "decision": "reject", "reason": "unsupported" }
                        })),
                    )
                },
            ),
        );
    let (base, _server) = reppo_server(app).await;
    let eval_id = client(base.clone()).submit(&submission()).await.unwrap();
    assert_eq!(eval_id, "ev_77");
    let status = client(base).status("ev_77").await.unwrap();
    let EvalStatus::Settled(settled) = status else {
        panic!("expected Settled");
    };
    assert_eq!(settled.reppo_score, 64.0);
    assert_eq!(settled.route.as_deref(), Some("review"));
}

#[tokio::test]
async fn real_client_maps_http_statuses_to_distinct_errors() {
    for (status, expected) in [
        (StatusCode::UNAUTHORIZED, "rejected the API key"),
        (StatusCode::PAYMENT_REQUIRED, "insufficient credits"),
        (StatusCode::NOT_FOUND, "evaluation not found"),
        (StatusCode::BAD_GATEWAY, "returned status 502"),
    ] {
        let app = axum::Router::new().route(
            "/agents/evals",
            post(move || async move { (status, Json(json!({}))) }),
        );
        let (base, _server) = reppo_server(app).await;
        let error = client(base).submit(&submission()).await.unwrap_err();
        assert!(error.to_string().contains(expected), "{error}");
    }
}

#[tokio::test]
async fn real_client_rejects_a_submit_response_without_an_id() {
    let app = axum::Router::new().route(
        "/agents/evals",
        post(|| async { (StatusCode::CREATED, Json(json!({ "status": "QUEUED" }))) }),
    );
    let (base, _server) = reppo_server(app).await;
    let ReppoError::Parse(message) = client(base).submit(&submission()).await.unwrap_err() else {
        panic!("expected Parse");
    };
    assert!(
        message.contains("id missing") && message.contains("\"status\":\"QUEUED\""),
        "{message}"
    );
}

#[tokio::test]
async fn an_unreadable_submit_body_is_logged_sanitized_so_the_id_can_be_recovered() {
    // Control characters first, then a recoverable id buried in a body that
    // is not the documented shape, then a long tail.
    let noisy = format!(
        "\u{7}\u{1b}[2J{{\"data\": {{\"idTypo\": \"ev_recover_me\"}}}}{}",
        "z".repeat(5_000)
    );
    let app = axum::Router::new().route(
        "/agents/evals",
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

    // Valid JSON that lacks `id` gets the same treatment.
    let app = axum::Router::new().route(
        "/agents/evals",
        post(|| async { (StatusCode::CREATED, Json(json!({ "evalId": "old-name" }))) }),
    );
    let (base, _server) = reppo_server(app).await;
    let ReppoError::Parse(message) = client(base).submit(&submission()).await.unwrap_err() else {
        panic!("expected Parse");
    };
    assert!(message.contains("id missing"), "{message}");
    assert!(message.contains("old-name"), "{message}");
}

fn rfc3339_overflowing_input_is_none_and_never_panics_body() {
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
fn rfc3339_overflowing_input_is_none_and_never_panics() {
    rfc3339_overflowing_input_is_none_and_never_panics_body();
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
async fn an_unreadable_status_body_is_refused() {
    let app = axum::Router::new().route(
        "/agents/evals/{eval_id}",
        get(|| async { (StatusCode::OK, "<html>not json</html>") }),
    );
    let (base, _server) = reppo_server(app).await;
    assert!(matches!(
        client(base).status("ev_1").await,
        Err(ReppoError::Parse(_))
    ));
}

fn padded_pending_body(total_bytes: usize) -> String {
    let core = r#"{"status":"QUEUED"}"#;
    format!("{core}{}", " ".repeat(total_bytes - core.len()))
}

#[tokio::test]
async fn a_response_body_over_one_mebibyte_is_refused_but_exactly_one_is_read() {
    let at_limit = padded_pending_body(MAX_BODY_BYTES);
    let over_limit = padded_pending_body(MAX_BODY_BYTES + 1);
    for (body, accepted) in [(at_limit, true), (over_limit, false)] {
        let app = axum::Router::new().route(
            "/agents/evals/{eval_id}",
            get(move || {
                let body = body.clone();
                async move { (StatusCode::OK, body) }
            }),
        );
        let (base, _server) = reppo_server(app).await;
        let result = client(base).status("ev_1").await;
        if accepted {
            assert_eq!(result.unwrap(), EvalStatus::Pending);
        } else {
            let Err(ReppoError::Parse(message)) = result else {
                panic!("expected Parse for an oversize body");
            };
            assert!(message.contains("exceeds"), "{message}");
        }
    }
}

#[tokio::test]
async fn an_oversize_submit_response_is_refused_too() {
    let app = axum::Router::new().route(
        "/agents/evals",
        post(|| async { (StatusCode::CREATED, "z".repeat(MAX_BODY_BYTES + 1)) }),
    );
    let (base, _server) = reppo_server(app).await;
    assert!(matches!(
        client(base).submit(&submission()).await,
        Err(ReppoError::Parse(_))
    ));
}

/// Streams a 2 MiB chunked response with no `Content-Length`, so the size
/// limit has to hold while the body is being read, not just from a header.
async fn chunked_oversize_server() -> (String, tokio::task::JoinHandle<()>) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                return;
            };
            tokio::spawn(async move {
                let mut seen = Vec::new();
                let mut buf = [0u8; 4096];
                while !seen.windows(4).any(|window| window == b"\r\n\r\n") {
                    match socket.read(&mut buf).await {
                        Ok(0) | Err(_) => return,
                        Ok(n) => seen.extend_from_slice(&buf[..n]),
                    }
                }
                let head = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\
                            Transfer-Encoding: chunked\r\nConnection: close\r\n\r\n";
                if socket.write_all(head.as_bytes()).await.is_err() {
                    return;
                }
                let chunk = vec![b' '; 64 * 1024];
                for _ in 0..32 {
                    let size_line = format!("{:x}\r\n", chunk.len());
                    if socket.write_all(size_line.as_bytes()).await.is_err()
                        || socket.write_all(&chunk).await.is_err()
                        || socket.write_all(b"\r\n").await.is_err()
                    {
                        return;
                    }
                }
                let _ = socket.write_all(b"0\r\n\r\n").await;
            });
        }
    });
    (format!("http://{addr}"), task)
}

#[tokio::test]
async fn a_chunked_body_without_a_length_is_cut_off_at_the_limit() {
    let (base, _server) = chunked_oversize_server().await;
    let Err(ReppoError::Parse(message)) = client(base).status("ev_1").await else {
        panic!("expected Parse for an oversize chunked body");
    };
    assert!(message.contains("exceeds"), "{message}");
}

#[test]
fn the_client_only_loads_when_a_key_exists_and_the_operator_enabled_it() {
    // A key sitting in `.env` must not by itself let the service spend
    // credits: the operator opts in with SOLV_REPPO_ENABLED=1 and sets an
    // explicit credit budget (SOLV_REPPO_STARTING_CREDITS).
    assert!(ReppoHttpClient::from_vars(Some("a-key"), Some("1")).is_some());
    for not_enabled in [
        None,
        Some(""),
        Some("0"),
        Some("true"),
        Some("yes"),
        Some(" 1"),
    ] {
        assert!(
            ReppoHttpClient::from_vars(Some("a-key"), not_enabled).is_none(),
            "enabled flag {not_enabled:?} must not enable Reppo"
        );
    }
    for no_key in [None, Some(""), Some("   ")] {
        assert!(
            ReppoHttpClient::from_vars(no_key, Some("1")).is_none(),
            "key {no_key:?} must not load"
        );
    }
}
