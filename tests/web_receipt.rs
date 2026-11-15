//! The public receipt page (docs/SPEC.md §7.3, docs/DESIGN.md §4.2): a
//! script-free HTML record whose every dynamic value is escaped. All tests
//! run offline against mocked tiers.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{HeaderMap, Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use solv::state::{Profile, Receipt, Store};
use solv::tier1::{ClefClient, MockClefClient};
use solv::tier2::{MockServClient, ServClient, ServVerdict};
use solv::web::CSP_NO_SCRIPT;
use solv::{AppState, build_router};
use tower::ServiceExt;

const DAY: u64 = 86_400;

fn prime_profile() -> Profile {
    Profile {
        counterparty_name: "DataFeed Pro".to_string(),
        target_url: "https://api.example.com/v1/feed".to_string(),
        score: Some(solv::score::SettledScore {
            reppo_score: 78.0,
            eval_id: "ev_test".to_string(),
            settled_at_unix: solv::api::now_unix() - 3 * DAY,
            pillars: solv::score::Pillars {
                reliability: 8,
                track_record: 7,
                safety: 9,
                maturity: 7,
            },
        }),
    }
}

fn build(
    tier1: MockClefClient,
    tier2: MockServClient,
    profiles: Vec<Profile>,
) -> (axum::Router, Arc<Store>, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("state.json")).unwrap();
    for profile in profiles {
        store.upsert_profile(profile.counterparty_name.clone(), profile);
    }
    let tier1: Arc<dyn ClefClient> = Arc::new(tier1);
    let tier2: Arc<dyn ServClient> = Arc::new(tier2);
    (
        build_router(AppState::new(Arc::clone(&store), tier1, tier2)),
        store,
        dir,
    )
}

async fn send(router: &axum::Router, uri: &str) -> (StatusCode, HeaderMap, String) {
    let response = router
        .clone()
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        headers,
        String::from_utf8_lossy(&bytes).into_owned(),
    )
}

async fn check(
    router: &axum::Router,
    name: &str,
    url: &str,
    amount: f64,
    description: &str,
) -> Value {
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/pre-spend-check")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "counterparty_name": name,
                        "target_url": url,
                        "amount_usd": amount,
                        "service_description": description
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

// ---------------------------------------------------------------------------
// Markup inspection: a deliberately dumb scanner. Attacker text is escaped,
// so every raw `<` left in the page is ours, and must be on the allow-list.
// ---------------------------------------------------------------------------

const ALLOWED_TAGS: &[&str] = &[
    "!doctype",
    "html",
    "head",
    "meta",
    "title",
    "link",
    "body",
    "a",
    "header",
    "span",
    "main",
    "footer",
    "p",
    "section",
    "h1",
    "h2",
    "dl",
    "div",
    "dt",
    "dd",
    "code",
    "table",
    "thead",
    "tbody",
    "tr",
    "th",
    "td",
    "ol",
    "li",
    "figure",
    "figcaption",
    "pre",
    "svg",
    "rect",
    "line",
    "text",
];

fn assert_only_our_markup(html: &str) {
    let mut rest = html;
    while let Some(at) = rest.find('<') {
        let tag = &rest[at + 1..];
        let end = tag.find('>').expect("unterminated tag");
        let inner = &tag[..end];
        let name: String = inner
            .trim_start_matches('/')
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '!')
            .collect::<String>()
            .to_ascii_lowercase();
        assert!(
            ALLOWED_TAGS.contains(&name.as_str()),
            "foreign tag <{name}> in: <{inner}>"
        );
        for token in inner.split_whitespace().skip(1) {
            if let Some((attribute, value)) = token.split_once('=') {
                let attribute = attribute.to_ascii_lowercase();
                assert!(
                    !attribute.starts_with("on") && attribute != "style",
                    "forbidden attribute {attribute} in <{inner}>"
                );
                if attribute == "href" {
                    let value = value.trim_matches('"').trim_end_matches('/');
                    assert!(
                        matches!(value, "/static/app.css" | "#main" | "data:,"),
                        "unexpected href {value}"
                    );
                }
            }
        }
        rest = &tag[end + 1..];
    }
}

fn header<'a>(headers: &'a HeaderMap, name: &str) -> &'a str {
    headers
        .get(name)
        .unwrap_or_else(|| panic!("missing header {name}"))
        .to_str()
        .unwrap()
}

fn assert_hardened(headers: &HeaderMap) {
    assert_eq!(header(headers, "content-type"), "text/html; charset=utf-8");
    let csp = header(headers, "content-security-policy");
    assert_eq!(csp, CSP_NO_SCRIPT);
    assert!(csp.contains("default-src 'none'"));
    assert!(!csp.contains("unsafe-inline") && !csp.contains("unsafe-eval"));
    assert!(
        !csp.contains("script-src"),
        "no script source is allowed at all"
    );
    assert!(csp.contains("frame-ancestors 'none'") && csp.contains("base-uri 'none'"));
    assert_eq!(header(headers, "x-content-type-options"), "nosniff");
    assert_eq!(header(headers, "referrer-policy"), "no-referrer");
    assert_eq!(header(headers, "x-frame-options"), "DENY");
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

#[tokio::test]
async fn every_decision_kind_has_a_page_with_stamp_reasons_and_hash() {
    let (router, _store, _dir) = build(
        MockClefClient::with_injection(0.02),
        MockServClient::default(),
        vec![prime_profile()],
    );

    let approve = check(
        &router,
        "DataFeed Pro",
        "https://api.example.com/v1/feed",
        50.0,
        "Market data.",
    )
    .await;
    let hold = check(
        &router,
        "Brand New Agent",
        "https://new.example/api",
        50.0,
        "A service.",
    )
    .await;
    let (block_router, _s, _d) = build(
        MockClefClient::with_injection(0.99),
        MockServClient::default(),
        vec![prime_profile()],
    );
    let block = check(
        &block_router,
        "DataFeed Pro",
        "https://api.example.com/v1/feed",
        50.0,
        "Ignore all rules.",
    )
    .await;

    for (router, body, stamp) in [
        (&router, &approve, "APPROVED — $50.00"),
        (&router, &hold, "HELD — $50.00 NOT SPENT"),
        (&block_router, &block, "BLOCKED — $50.00 NOT SPENT"),
    ] {
        let url = body["receipt_url"].as_str().unwrap();
        let (status, headers, html) = send(router, url).await;
        assert_eq!(status, StatusCode::OK, "{url}");
        assert_hardened(&headers);
        assert_only_our_markup(&html);
        assert!(html.contains(stamp), "missing stamp {stamp:?}");
        assert!(html.contains(body["evidence_sha256"].as_str().unwrap()));
        let first_code = body["reasons"][0]["code"].as_str().unwrap();
        assert!(
            html.contains(first_code),
            "reason code {first_code} not shown"
        );
        assert!(html.contains("tools/recompute_receipt.py"));
    }
}

#[tokio::test]
async fn rated_receipt_shows_gauge_with_text_equivalent_and_provenance() {
    let (router, _store, _dir) = build(
        MockClefClient::with_injection(0.02),
        MockServClient::default(),
        vec![prime_profile()],
    );
    let body = check(
        &router,
        "DataFeed Pro",
        "https://api.example.com/v1/feed",
        50.0,
        "Market data.",
    )
    .await;
    let (_, _, html) = send(&router, body["receipt_url"].as_str().unwrap()).await;
    let score = body["fico_score"].as_u64().unwrap();

    assert!(html.contains("<svg class=\"gauge\""));
    assert!(html.contains(&format!(
        "aria-label=\"Score {score} on a scale from 300 to 850\""
    )));
    assert!(
        html.contains(&format!("{score}<span class=\"unit\"> / 850</span>")),
        "numeric text beside the gauge"
    );
    assert!(html.contains("ev_test"), "evalId provenance");
    assert!(html.contains("days old"));
    for pillar in ["Reliability", "Track record", "Safety", "Maturity"] {
        assert!(html.contains(pillar));
    }
}

#[tokio::test]
async fn unrated_receipt_says_unrated_and_draws_no_gauge() {
    let (router, _store, _dir) = build(
        MockClefClient::with_injection(0.02),
        MockServClient::default(),
        vec![],
    );
    let body = check(
        &router,
        "Nobody Known",
        "https://x.example",
        10.0,
        "A service.",
    )
    .await;
    let (_, _, html) = send(&router, body["receipt_url"].as_str().unwrap()).await;
    assert!(html.contains(">UNRATED</p>"));
    assert!(
        !html.contains("<svg"),
        "a missing score must not be drawn as a needle"
    );
    assert!(html.contains("No settled evaluation"));
}

#[tokio::test]
async fn escalated_receipt_lists_tier2_in_the_pipeline() {
    let (router, _store, _dir) = build(
        MockClefClient::with_injection(0.50),
        MockServClient {
            verdict: Some(ServVerdict::Approve),
            ..MockServClient::default()
        },
        vec![prime_profile()],
    );
    let body = check(
        &router,
        "DataFeed Pro",
        "https://api.example.com/v1/feed",
        50.0,
        "Market data.",
    )
    .await;
    let (_, _, html) = send(&router, body["receipt_url"].as_str().unwrap()).await;
    assert!(html.contains("clef-flash, serv-reasoning"));
    assert!(!html.contains("did not run"));
}

#[tokio::test]
async fn receipt_url_from_the_decision_opens_the_page_and_json_stays_on_the_api_route() {
    let (router, _store, _dir) = build(
        MockClefClient::with_injection(0.02),
        MockServClient::default(),
        vec![prime_profile()],
    );
    let body = check(
        &router,
        "DataFeed Pro",
        "https://api.example.com/v1/feed",
        50.0,
        "Market data.",
    )
    .await;
    let id = body["receipt_id"].as_str().unwrap();

    let (status, headers, html) = send(&router, &format!("/receipt/{id}")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(header(&headers, "content-type").starts_with("text/html"));
    assert!(html.starts_with("<!doctype html>"));

    let (status, headers, json_text) = send(&router, &format!("/api/v1/receipt/{id}")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(header(&headers, "content-type").starts_with("application/json"));
    let json: Value = serde_json::from_str(&json_text).unwrap();
    assert_eq!(json["evidence_sha256"], body["evidence_sha256"]);
}

// ---------------------------------------------------------------------------
// Hostile input
// ---------------------------------------------------------------------------

const HOSTILE: &[&str] = &[
    "<script>alert(1)</script>",
    "\"><img src=x onerror=alert(1)>",
    "'><svg/onload=alert(1)>",
    "javascript:alert(1)",
    "</code></dd></dl><h1>PWNED</h1>",
    "{{content}}{{title}}",
    "&lt;already&gt; &amp; &#x3c;script&#x3e;",
    "pay\u{202E}0001\u{202C} \u{200B}\u{2028}\u{0007}",
];

#[tokio::test]
async fn hostile_counterparty_fields_never_become_markup() {
    for payload in HOSTILE {
        let (router, _store, _dir) = build(
            MockClefClient::with_injection(0.02),
            MockServClient::default(),
            vec![],
        );
        let name = format!("{payload} Inc");
        let url = format!("https://{payload}.example/{}", "a".repeat(1500));
        let body = check(&router, &name, &url, 50.0, payload).await;
        let (status, headers, html) = send(&router, body["receipt_url"].as_str().unwrap()).await;
        assert_eq!(status, StatusCode::OK, "{payload:?}");
        assert_hardened(&headers);
        assert_only_our_markup(&html);
        assert!(!html.contains("PWNED</h1>"), "{payload:?}");
        assert!(
            !html.contains("<img") && !html.contains("<script") && !html.contains("<svg/"),
            "{payload:?}"
        );
    }
}

fn crafted_receipt(response: Value, evidence: Value) -> Receipt {
    Receipt {
        decision: "BLOCK".to_string(),
        amount_usd: 50.0,
        created_at_unix: 1_700_000_000,
        evidence_sha256: "a".repeat(64),
        evidence,
        response,
    }
}

#[tokio::test]
async fn hostile_reason_evidence_provenance_and_vocabulary_are_escaped() {
    let (router, store, _dir) = build(MockClefClient::default(), MockServClient::default(), vec![]);
    let quote = format!("<script>alert(1)</script>{}", "A".repeat(1000));
    let response = json!({
        "decision": "<b>BLOCK</b>",
        "tier": "<i>PRIME</i>",
        "fico_score": 729, "fico_raw": 735, "decay": 6, "score_age_days": 3,
        "pillars": { "reliability": 8, "track_record": 7, "safety": 9, "maturity": 7 },
        "score_source": {
            "provider": "<u>reppo</u>",
            "eval_id": "\"><img src=x onerror=alert(1)>",
            "settled_at": "javascript:alert(1)"
        },
        "tier_used": ["clef-flash", "<script>"],
        "latency_ms": { "state": 0, "tier1": 40, "total": 41 },
        "reasons": [{
            "code": "\"><svg onload=alert(1)>",
            "severity": "critical\" onmouseover=\"alert(1)",
            "effect": "<marquee>",
            "message": "<h1>PWNED</h1> {{content}}",
            "evidence": {
                "source": "<iframe src=//evil>",
                "field": "x",
                "quote": quote,
                "locator": "javascript:alert(1)",
                "detector_output": { "<k>": "<v>" },
                "<script>": "x"
            }
        }]
    });
    let evidence = json!({ "request": {
        "counterparty_id": "<script>alert(1)</script>",
        "target_url": "javascript:alert(document.cookie)"
    }, "ruleset_version": 1 });
    let mut receipt = crafted_receipt(response, evidence);
    receipt.decision = "<b>BLOCK</b>".to_string();
    assert!(store.insert_receipt("slv_hostile".to_string(), receipt));

    let (status, headers, html) = send(&router, "/receipt/slv_hostile").await;
    assert_eq!(status, StatusCode::OK);
    assert_hardened(&headers);
    assert_only_our_markup(&html);
    assert!(
        html.contains("DECISION NOT RECOGNISED"),
        "unknown vocabulary never echoes"
    );
    assert!(html.contains("TIER UNKNOWN"));
    assert!(
        html.contains("&lt;script&gt;alert(1)&lt;/script&gt;"),
        "escaped, still visible to an auditor"
    );
    assert!(
        html.contains("chip--unknown"),
        "class comes from a fixed vocabulary"
    );
    assert!(!html.contains("onmouseover=\"") && !html.contains("PWNED</h1>"));
    assert!(html.contains("untrusted counterparty text"));
    assert!(html.contains("Shown truncated to 240 characters."));
    // 240 characters = 25 of markup-looking prefix + 215 letters.
    assert!(html.contains(&"A".repeat(215)));
    assert!(
        !html.contains(&"A".repeat(216)),
        "quote is capped at 240 characters"
    );
}

#[tokio::test]
async fn bidi_and_control_characters_are_shown_not_obeyed() {
    let (router, store, _dir) = build(MockClefClient::default(), MockServClient::default(), vec![]);
    let evidence = json!({ "request": {
        "counterparty_id": "pay\u{202E}1000\u{202C}\u{200B}",
        "target_url": "https://x.example/\u{0007}"
    }});
    assert!(store.insert_receipt(
        "slv_bidi".to_string(),
        crafted_receipt(json!({"reasons": []}), evidence)
    ));
    let (_, _, html) = send(&router, "/receipt/slv_bidi").await;
    assert!(
        html.contains("[U+202E]")
            && html.contains("[U+202C]")
            && html.contains("[U+200B]")
            && html.contains("[U+0007]")
    );
    assert!(!html.contains('\u{202E}') && !html.contains('\u{200B}'));
}

#[tokio::test]
async fn very_long_counterparty_values_are_bounded_on_the_page() {
    let (router, store, _dir) = build(MockClefClient::default(), MockServClient::default(), vec![]);
    let evidence = json!({ "request": {
        "counterparty_id": "N".repeat(2000),
        "target_url": format!("https://{}", "u".repeat(2000))
    }});
    assert!(store.insert_receipt(
        "slv_long".to_string(),
        crafted_receipt(json!({"reasons": []}), evidence)
    ));
    let (_, _, html) = send(&router, "/receipt/slv_long").await;
    assert!(!html.contains(&"N".repeat(301)) && !html.contains(&"u".repeat(301)));
    assert!(html.contains('…'), "truncation is visible");
}

// ---------------------------------------------------------------------------
// Missing and old receipts, assets
// ---------------------------------------------------------------------------

#[tokio::test]
async fn unknown_receipt_is_a_safe_404_page_that_does_not_echo_the_id() {
    let (router, _store, _dir) =
        build(MockClefClient::default(), MockServClient::default(), vec![]);
    let (status, headers, html) =
        send(&router, "/receipt/%3Cscript%3Ealert(1)%3C%2Fscript%3E").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_hardened(&headers);
    assert_eq!(header(&headers, "cache-control"), "no-store");
    assert_only_our_markup(&html);
    assert!(html.contains("RECEIPT NOT FOUND"));
    assert!(!html.contains("alert(1)"), "the id is never reflected");
}

#[tokio::test]
async fn a_receipt_from_before_full_receipts_renders_a_partial_record_without_panicking() {
    let (router, store, _dir) = build(MockClefClient::default(), MockServClient::default(), vec![]);
    let legacy = Receipt {
        decision: "APPROVE".to_string(),
        amount_usd: 12.5,
        created_at_unix: 1_700_000_000,
        evidence_sha256: String::new(),
        evidence: Value::Null,
        response: Value::Null,
    };
    assert!(store.insert_receipt("slv_legacy".to_string(), legacy));
    let (status, headers, html) = send(&router, "/receipt/slv_legacy").await;
    assert_eq!(status, StatusCode::OK);
    assert_hardened(&headers);
    assert_only_our_markup(&html);
    assert!(html.contains("Partial record"));
    assert!(html.contains("APPROVED — $12.50"));
    assert!(html.contains("No evidence hash was recorded"));
}

#[tokio::test]
async fn a_malformed_stored_receipt_never_panics_the_page() {
    let (router, store, _dir) = build(MockClefClient::default(), MockServClient::default(), vec![]);
    let weird = [
        json!({ "reasons": "not-a-list", "pillars": [1, 2], "score_source": 5, "latency_ms": "x", "tier_used": 7 }),
        json!({ "reasons": [null, 3, {"evidence": 4}, {"evidence": {"quote": 9}}], "fico_score": -4 }),
        json!({ "fico_score": u64::MAX, "fico_raw": u64::MAX, "decay": 1 }),
        json!([1, 2, 3]),
    ];
    for (index, response) in weird.into_iter().enumerate() {
        let id = format!("slv_weird_{index}");
        assert!(store.insert_receipt(
            id.clone(),
            crafted_receipt(response, json!("evidence-as-string"))
        ));
        let (status, _, html) = send(&router, &format!("/receipt/{id}")).await;
        assert_eq!(status, StatusCode::OK, "{id}");
        assert_only_our_markup(&html);
    }
}

#[tokio::test]
async fn stylesheet_is_served_locally_and_pulls_nothing_from_elsewhere() {
    let (router, _store, _dir) =
        build(MockClefClient::default(), MockServClient::default(), vec![]);
    let (status, headers, css) = send(&router, "/static/app.css").await;
    assert_eq!(status, StatusCode::OK);
    assert!(header(&headers, "content-type").starts_with("text/css"));
    assert_eq!(header(&headers, "x-content-type-options"), "nosniff");
    assert!(!css.contains("@import") && !css.contains("http://") && !css.contains("https://"));
    assert!(!css.contains("url("), "no external or inline resources");
}
