//! The public receipt page (docs/SPEC.md §7.3, docs/DESIGN.md §4.2): a
//! server-rendered HTML record whose every dynamic value is escaped. Its only
//! script is the same-origin hash verifier, and only on a receipt that has
//! something to verify. All tests run offline against mocked tiers.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{HeaderMap, Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use solv::state::{Profile, Receipt, Store};
use solv::tier1::{ClefClient, MockClefClient};
use solv::tier2::{MockServClient, ServClient, ServVerdict};
use solv::web::{CSP_NO_SCRIPT, CSP_RECEIPT_VERIFY};
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

/// The one script tag a receipt page may contain: a same-origin asset with a
/// hex version and no inline body.
fn is_verify_script_open_tag(inner: &str) -> bool {
    inner
        .strip_prefix("script src=\"/static/receipt-verify.js?v=")
        .and_then(|rest| rest.strip_suffix('"'))
        .is_some_and(|version| {
            !version.is_empty() && version.chars().all(|c| c.is_ascii_hexdigit())
        })
}

fn assert_only_our_markup(html: &str) {
    let mut rest = html;
    let mut open_script = false;
    while let Some(at) = rest.find('<') {
        let tag = &rest[at + 1..];
        let end = tag.find('>').expect("unterminated tag");
        let inner = &tag[..end];
        if open_script {
            assert_eq!(
                (&rest[..at], inner),
                ("", "/script"),
                "the verifier script has no inline body"
            );
            open_script = false;
            rest = &tag[end + 1..];
            continue;
        }
        if inner.starts_with("script") {
            assert!(
                is_verify_script_open_tag(inner),
                "only the verifier asset may be a script: <{inner}>"
            );
            open_script = true;
            rest = &tag[end + 1..];
            continue;
        }
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
                    !attribute.starts_with("on") && attribute != "style" && attribute != "src",
                    "forbidden attribute {attribute} in <{inner}>"
                );
                if attribute == "href" {
                    let value = value.trim_matches('"').trim_end_matches('/');
                    assert!(
                        value.strip_prefix("/static/app.css?v=").is_some_and(
                            |v| !v.is_empty() && v.chars().all(|c| c.is_ascii_hexdigit())
                        ) || matches!(value, "#main" | "data:,"),
                        "unexpected href {value}"
                    );
                }
            }
        }
        rest = &tag[end + 1..];
    }
    assert!(!open_script, "unterminated verifier script");
}

fn header<'a>(headers: &'a HeaderMap, name: &str) -> &'a str {
    headers
        .get(name)
        .unwrap_or_else(|| panic!("missing header {name}"))
        .to_str()
        .unwrap()
}

/// The page without the hidden canonical-evidence block, which carries the
/// exact hashed bytes (raw bidi characters, untruncated values) and is never
/// shown. Every claim about what a reader sees is made on this.
fn visible_page(html: &str) -> String {
    let open = "<pre id=\"canonical-evidence\" hidden>";
    match html.find(open) {
        Some(start) => {
            let end = start + html[start..].find("</pre>").expect("block end") + "</pre>".len();
            format!("{}{}", &html[..start], &html[end..])
        }
        None => html.to_string(),
    }
}

fn assert_hardened(headers: &HeaderMap) {
    assert_eq!(header(headers, "content-type"), "text/html; charset=utf-8");
    let csp = header(headers, "content-security-policy");
    assert!(
        csp == CSP_NO_SCRIPT || csp == CSP_RECEIPT_VERIFY,
        "only the two known policies may appear: {csp}"
    );
    assert!(csp.contains("default-src 'none'"));
    assert!(!csp.contains("unsafe-inline") && !csp.contains("unsafe-eval"));
    assert!(
        !csp.contains("connect-src"),
        "the verifier makes no request"
    );
    assert!(
        csp == CSP_NO_SCRIPT || csp.contains("script-src 'self';"),
        "a script policy allows this origin and nothing else"
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
        (&router, &hold, "HELD — $50.00 NOT AUTHORISED"),
        (&block_router, &block, "BLOCKED — $50.00 NOT AUTHORISED"),
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
            !html.contains("<img") && !html.contains("<script>") && !html.contains("<svg/"),
            "{payload:?}"
        );
        assert_eq!(
            html.matches("<script").count(),
            1,
            "{payload:?}: only the verifier asset tag, never a payload"
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
    let html = visible_page(&html);
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
    let html = visible_page(&html);
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

// ---------------------------------------------------------------------------
// Review fixes
// ---------------------------------------------------------------------------

/// Every `="..."` attribute value on the page.
fn attribute_values(html: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut rest = html;
    while let Some(at) = rest.find("=\"") {
        let after = &rest[at + 2..];
        let end = after.find('"').expect("unterminated attribute value");
        values.push(after[..end].to_string());
        rest = &after[end + 1..];
    }
    values
}

#[tokio::test]
async fn slot_markers_in_the_receipt_id_cannot_pull_the_body_into_the_title() {
    let (router, store, _dir) = build(MockClefClient::default(), MockServClient::default(), vec![]);
    let id = "{{content}}<script>alert(1)</script>{{footer}}{{scripts}}{{head_extra}}{{title}}";
    assert!(store.insert_receipt(
        id.to_string(),
        crafted_receipt(json!({"reasons": []}), json!({}))
    ));
    let uri = format!(
        "/receipt/{}",
        id.replace('{', "%7B")
            .replace('}', "%7D")
            .replace('<', "%3C")
            .replace('>', "%3E")
            .replace('/', "%2F")
    );
    let (status, _, html) = send(&router, &uri).await;
    assert_eq!(status, StatusCode::OK);
    assert_only_our_markup(&html);

    let title_start = html.find("<title>").unwrap() + 7;
    let title_end = html.find("</title>").unwrap();
    let title = &html[title_start..title_end];
    assert!(!title.contains('<'), "title holds only text: {title}");
    assert_eq!(html.matches("<main").count(), 1);
    assert_eq!(html.matches("<footer").count(), 1);
    assert!(!html.contains("<script>"), "the id never becomes markup");
    assert_eq!(
        html.matches("<script").count(),
        1,
        "only the verifier asset tag"
    );
}

#[tokio::test]
async fn stamp_is_followed_by_the_not_an_authorisation_sentence_for_every_decision() {
    let (router, store, _dir) = build(MockClefClient::default(), MockServClient::default(), vec![]);
    for (decision, stamp) in [
        ("APPROVE", "APPROVED — $50.00"),
        ("HOLD", "HELD — $50.00 NOT AUTHORISED"),
        ("BLOCK", "BLOCKED — $50.00 NOT AUTHORISED"),
    ] {
        let mut receipt = crafted_receipt(json!({"reasons": []}), json!({}));
        receipt.decision = decision.to_string();
        let id = format!("slv_stamp_{decision}");
        assert!(store.insert_receipt(id.clone(), receipt));
        let (_, _, html) = send(&router, &format!("/receipt/{id}")).await;
        let after_stamp = &html[html.find(stamp).expect(stamp)..];
        let sentence =
            "Solv&#39;s recorded decision for this request. Not an authorisation to pay.";
        let at = after_stamp.find(sentence).expect("sentence shown");
        assert!(
            at < 200,
            "{decision}: the sentence sits right under the stamp"
        );
        assert!(
            !html.contains("NOT SPENT"),
            "Solv cannot know whether money moved"
        );
    }
}

#[tokio::test]
async fn hash_section_states_its_scope_and_the_full_canonical_recipe() {
    let (router, store, _dir) = build(MockClefClient::default(), MockServClient::default(), vec![]);
    let receipt = crafted_receipt(json!({"reasons": []}), json!({"request": {}}));
    assert!(store.insert_receipt("slv_hashdoc".to_string(), receipt));
    let (_, _, html) = send(&router, "/receipt/slv_hashdoc").await;
    for needle in [
        "does not cover the receipt id or the issue time",
        "keys sorted by UTF-8 bytes",
        "no whitespace between tokens",
        "non-ASCII text left as raw UTF-8",
        "Python's <code>repr</code>",
        "tools/recompute_receipt.py",
    ] {
        assert!(html.contains(needle), "missing {needle:?}");
    }
}

#[tokio::test]
async fn no_dynamic_value_ever_lands_in_an_attribute() {
    let (router, store, _dir) = build(MockClefClient::default(), MockServClient::default(), vec![]);
    let m = "ZZMARK";
    let response = json!({
        "decision": m, "tier": m, "fico_score": 729, "fico_raw": 735, "decay": 6, "score_age_days": 3,
        "pillars": { "reliability": 8, "track_record": 7, "safety": 9, "maturity": 7 },
        "score_source": { "provider": m, "eval_id": m, "settled_at": m },
        "tier_used": [m],
        "latency_ms": { "state": 0, "tier1": 40, "total": 41 },
        "reasons": [{ "code": m, "severity": m, "effect": m, "message": m,
            "evidence": { "source": m, "field": m, "quote": m, "locator": m, m: m } }]
    });
    let evidence =
        json!({ "request": { "counterparty_id": m, "target_url": m }, "ruleset_version": 1 });
    assert!(store.insert_receipt(format!("slv_{m}"), crafted_receipt(response, evidence)));
    let (_, _, html) = send(&router, &format!("/receipt/slv_{m}")).await;
    assert!(html.contains(m), "the marker is rendered as text");
    for value in attribute_values(&html) {
        assert!(!value.contains(m), "dynamic value in attribute: {value}");
    }
}

#[tokio::test]
async fn gauge_label_matches_the_visible_score_even_out_of_range() {
    let (router, store, _dir) = build(MockClefClient::default(), MockServClient::default(), vec![]);
    let response =
        json!({ "tier": "PRIME", "fico_score": 900, "fico_raw": 900, "decay": 0, "reasons": [] });
    assert!(store.insert_receipt("slv_over".to_string(), crafted_receipt(response, json!({}))));
    let (_, _, html) = send(&router, "/receipt/slv_over").await;
    assert!(html.contains("900<span class=\"unit\"> / 850</span>"));
    assert!(html.contains("aria-label=\"Score 900 on a scale from 300 to 850\""));
}

#[tokio::test]
async fn fair_and_good_do_not_borrow_the_hold_colour() {
    let (router, store, _dir) = build(MockClefClient::default(), MockServClient::default(), vec![]);
    for tier in ["FAIR", "GOOD"] {
        let response = json!({ "tier": tier, "fico_score": 600, "reasons": [] });
        let id = format!("slv_{tier}");
        assert!(store.insert_receipt(id.clone(), crafted_receipt(response, json!({}))));
        let (_, _, html) = send(&router, &format!("/receipt/{id}")).await;
        assert!(html.contains(&format!("badge--neutral\">{tier}")), "{tier}");
        assert!(!html.contains(&format!("badge--hold\">{tier}")), "{tier}");
    }
}

#[tokio::test]
async fn amounts_format_honestly_for_negative_and_denormal_values() {
    let (router, store, _dir) = build(MockClefClient::default(), MockServClient::default(), vec![]);
    for (amount, expected) in [(-50.0, "-$50.00"), (5e-324, "$5e-324")] {
        let mut receipt = crafted_receipt(json!({"reasons": []}), json!({}));
        receipt.decision = "APPROVE".to_string();
        receipt.amount_usd = amount;
        let id = format!("slv_amt_{}", amount.abs().to_bits());
        assert!(store.insert_receipt(id.clone(), receipt));
        let (_, _, html) = send(&router, &format!("/receipt/{id}")).await;
        assert!(
            html.contains(&format!("APPROVED — {expected}")),
            "{amount}: {expected}"
        );
        assert!(!html.contains("$-") && !html.contains("$0.<"));
    }
}

// ---------------------------------------------------------------------------
// Shared shell
// ---------------------------------------------------------------------------

#[tokio::test]
async fn stylesheet_url_is_versioned_by_content_and_static_routes_are_table_driven() {
    let (router, store, _dir) = build(MockClefClient::default(), MockServClient::default(), vec![]);
    assert!(store.insert_receipt(
        "slv_css".to_string(),
        crafted_receipt(json!({"reasons": []}), json!({}))
    ));
    let (_, _, html) = send(&router, "/receipt/slv_css").await;
    let at = html.find("/static/app.css?v=").expect("versioned href");
    let version: String = html[at + 18..]
        .chars()
        .take_while(|c| c.is_ascii_hexdigit())
        .collect();
    assert!(version.len() >= 12, "{version}");

    let (status, _, css) = send(&router, &format!("/static/app.css?v={version}")).await;
    assert_eq!(status, StatusCode::OK);
    let expected = solv::receipt::sha256_hex(css.as_bytes());
    assert!(
        expected.starts_with(&version),
        "version is the SHA-256 prefix of the served bytes"
    );

    for uri in [
        "/static/missing.css",
        "/static/..%2FCargo.toml",
        "/static/app.css%2F..%2F..%2FCargo.toml",
        "/static/%2e%2e/Cargo.toml",
    ] {
        let (status, headers, body) = send(&router, uri).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{uri}");
        assert_eq!(header(&headers, "x-content-type-options"), "nosniff");
        assert!(
            !body.contains("[package]"),
            "{uri} must never reach the filesystem"
        );
    }

    // The bare directory matches no asset route at all and falls through to
    // the router's default 404, which has an empty body.
    let (status, _, body) = send(&router, "/static/").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(body.is_empty());
}

#[tokio::test]
async fn shell_slots_for_head_footer_and_scripts_are_empty_or_filled_never_leaked() {
    let (router, store, _dir) = build(MockClefClient::default(), MockServClient::default(), vec![]);
    assert!(store.insert_receipt(
        "slv_slots".to_string(),
        crafted_receipt(json!({"reasons": []}), json!({}))
    ));
    let (_, _, html) = send(&router, "/receipt/slv_slots").await;
    assert!(!html.contains("{{"), "no unfilled slot marker");
    assert!(html.contains("A receipt is a record of a decision."));
    assert_eq!(
        html.matches("<script src=\"/static/receipt-verify.js?v=")
            .count(),
        1,
        "the scripts slot holds the verifier tag and nothing else"
    );
}

// ---------------------------------------------------------------------------
// In-browser hash verifier (static/receipt-verify.js)
// ---------------------------------------------------------------------------

/// Decodes exactly the entities `escape_exact` emits, in one left-to-right
/// pass, as an HTML parser's `textContent` would.
fn decode_exact_entities(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        let after = &rest[at..];
        let (decoded, len) = [
            ("&amp;", '&'),
            ("&lt;", '<'),
            ("&gt;", '>'),
            ("&quot;", '"'),
            ("&#39;", '\''),
            ("&#96;", '`'),
        ]
        .iter()
        .find(|(entity, _)| after.starts_with(entity))
        .map(|(entity, c)| (*c, entity.len()))
        .unwrap_or_else(|| panic!("unexpected entity at: {}", &after[..after.len().min(12)]));
        out.push(decoded);
        rest = &after[len..];
    }
    out.push_str(rest);
    out
}

/// The text of `<pre id="canonical-evidence" hidden>…</pre>`, still entity-encoded.
fn embedded_canonical(html: &str) -> &str {
    let open = "<pre id=\"canonical-evidence\" hidden>";
    let start = html.find(open).expect("canonical block") + open.len();
    let end = start + html[start..].find("</pre>").expect("block end");
    &html[start..end]
}

fn printed_hash(html: &str) -> String {
    let open = "id=\"evidence-hash\">";
    let start = html.find(open).expect("printed hash") + open.len();
    let end = start + html[start..].find("</p>").expect("hash end");
    html[start..end].to_string()
}

fn verifiable_receipt(evidence: Value) -> Receipt {
    Receipt {
        decision: "BLOCK".to_string(),
        amount_usd: 50.0,
        created_at_unix: 1_700_000_000,
        evidence_sha256: solv::receipt::evidence_sha256(&evidence),
        evidence,
        response: json!({"reasons": []}),
    }
}

#[tokio::test]
async fn a_verifiable_receipt_gets_the_narrow_script_policy_and_the_verifier_markup() {
    let (router, store, _dir) = build(MockClefClient::default(), MockServClient::default(), vec![]);
    assert!(store.insert_receipt(
        "slv_verify".to_string(),
        verifiable_receipt(json!({"request": {"counterparty_id": "a"}, "ruleset_version": 2}))
    ));
    let (status, headers, html) = send(&router, "/receipt/slv_verify").await;
    assert_eq!(status, StatusCode::OK);
    assert_hardened(&headers);
    let csp = header(&headers, "content-security-policy");
    assert_eq!(csp, CSP_RECEIPT_VERIFY);
    assert_eq!(
        csp.replacen("script-src 'self'; ", "", 1),
        CSP_NO_SCRIPT,
        "the only difference from the script-free policy is script-src 'self'"
    );
    assert_only_our_markup(&html);

    for marker in [
        "id=\"verify-slot\"",
        "id=\"canonical-evidence\" hidden",
        "id=\"evidence-hash\"",
    ] {
        assert_eq!(html.matches(marker).count(), 1, "{marker}");
    }

    // The tag's version is the content hash of the bytes the server serves.
    let (status, asset_headers, js) = send(&router, "/static/receipt-verify.js").await;
    assert_eq!(status, StatusCode::OK);
    assert!(header(&asset_headers, "content-type").starts_with("text/javascript"));
    assert_eq!(header(&asset_headers, "x-content-type-options"), "nosniff");
    let version = &solv::receipt::sha256_hex(js.as_bytes())[..16];
    assert!(
        html.contains(&format!(
            "<script src=\"/static/receipt-verify.js?v={version}\"></script>"
        )),
        "script tag must carry the content version"
    );
}

#[tokio::test]
async fn receipts_with_nothing_to_verify_keep_the_script_free_policy() {
    let (router, store, _dir) = build(MockClefClient::default(), MockServClient::default(), vec![]);
    let mut partial = verifiable_receipt(Value::Null);
    partial.response = Value::Null;
    let mut no_hash = verifiable_receipt(json!({"request": {}}));
    no_hash.evidence_sha256 = String::new();
    assert!(store.insert_receipt("slv_partial".to_string(), partial));
    assert!(store.insert_receipt("slv_nohash".to_string(), no_hash));
    for uri in [
        "/receipt/slv_partial",
        "/receipt/slv_nohash",
        "/receipt/missing",
    ] {
        let (_, headers, html) = send(&router, uri).await;
        assert_eq!(
            header(&headers, "content-security-policy"),
            CSP_NO_SCRIPT,
            "{uri}"
        );
        assert!(!html.contains("<script"), "{uri}");
        assert!(!html.contains("canonical-evidence"), "{uri}");
    }
}

#[tokio::test]
async fn the_embedded_text_is_exactly_the_hashed_bytes_even_with_hostile_characters() {
    let (router, store, _dir) = build(MockClefClient::default(), MockServClient::default(), vec![]);
    // Raw bidi/zero-width/C1/line-separator characters stay raw in canonical
    // JSON (ensure_ascii=False); markup, entity look-alikes, quote characters,
    // a literal `[U+202E]`, and floats Python writes differently from
    // JSON.stringify all have to survive the trip into the page unchanged.
    let evidence = json!({
        "request": {
            "counterparty_id": "<script>alert(1)</script>\u{202E}pay\u{202C}\u{200B}\u{FEFF}\u{2028}\u{0085}",
            "target_url": "https://x.example/?a=1&b=<b>\"q\"'s`t`",
            "note": "&lt;already&gt; [U+202E] \\u202e é 日本 😀"
        },
        "numbers": [1.0, 1e-7, 1e22, 0.1, 100, -0.0],
        "ruleset_version": 2
    });
    let receipt = verifiable_receipt(evidence.clone());
    let expected_hash = receipt.evidence_sha256.clone();
    assert!(store.insert_receipt("slv_exact".to_string(), receipt));
    let (_, headers, html) = send(&router, "/receipt/slv_exact").await;
    assert_hardened(&headers);
    assert_only_our_markup(&html);

    let block = embedded_canonical(&html);
    assert!(
        !block.contains('<'),
        "no raw markup inside the hashed block"
    );
    let decoded = decode_exact_entities(block);
    assert_eq!(decoded, solv::receipt::canonical_json(&evidence));
    assert!(
        decoded.contains("1.0") && decoded.contains("1e-07"),
        "Python float forms"
    );
    assert!(
        decoded.contains('\u{202E}'),
        "the raw bidi character is preserved"
    );
    assert_eq!(solv::receipt::sha256_hex(decoded.as_bytes()), expected_hash);
    assert_eq!(printed_hash(&html), expected_hash);

    // The visible parts of the page still show the invisible characters as
    // markers; only the hidden block carries them raw.
    let visible = html.replace(block, "");
    assert!(
        !visible.contains('\u{202E}'),
        "no raw bidi outside the hidden block"
    );
}

#[tokio::test]
async fn no_inline_script_and_no_event_handler_can_appear_on_a_receipt_page() {
    let (router, store, _dir) = build(MockClefClient::default(), MockServClient::default(), vec![]);
    let m = "\"><script>alert(1)</script><img src=x onerror=alert(2)><svg onload=alert(3)>";
    let evidence =
        json!({"request": {"counterparty_id": m, "target_url": m}, "ruleset_version": 2});
    let mut receipt = verifiable_receipt(evidence);
    receipt.response = json!({
        "decision": m, "tier": m, "reasons": [{"code": m, "severity": m, "effect": m,
            "message": m, "evidence": {"source": m, "field": m, "quote": m, "locator": m}}]
    });
    assert!(store.insert_receipt("slv_nohandlers".to_string(), receipt));
    let (_, headers, html) = send(&router, "/receipt/slv_nohandlers").await;
    assert_hardened(&headers);
    assert_only_our_markup(&html);
    assert_eq!(
        html.matches("<script").count(),
        1,
        "exactly one script tag: the verifier asset"
    );
    assert_eq!(html.matches("</script>").count(), 1);
    assert!(
        !html.contains("<img") && !html.contains("<svg onload") && !html.contains("<script>"),
        "hostile markup stayed text"
    );
    // Every tag, not only the ones the checker names, is free of handlers
    // and script URLs.
    let mut rest = html.as_str();
    while let Some(at) = rest.find('<') {
        let tag = &rest[at + 1..];
        let end = tag.find('>').unwrap();
        let inner = tag[..end].to_ascii_lowercase();
        assert!(
            !inner
                .split_whitespace()
                .skip(1)
                .any(|t| t.starts_with("on")),
            "event handler in <{inner}>"
        );
        assert!(!inner.contains("javascript:"), "script URL in <{inner}>");
        rest = &tag[end + 1..];
    }
}

#[tokio::test]
async fn the_verifier_script_makes_no_request_and_builds_no_markup() {
    let (router, _store, _dir) =
        build(MockClefClient::default(), MockServClient::default(), vec![]);
    let (status, headers, js) = send(&router, "/static/receipt-verify.js").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        header(&headers, "content-security-policy"),
        "default-src 'none'; script-src 'self'"
    );
    for forbidden in [
        "eval(",
        "Function(",
        "innerHTML",
        "outerHTML",
        "insertAdjacentHTML",
        "document.write",
        "fetch(",
        "XMLHttpRequest",
        "WebSocket",
        "sendBeacon",
        "import(",
        "importScripts",
        "location",
        "http://",
        "https://",
    ] {
        assert!(
            !js.contains(forbidden),
            "verifier script must not use {forbidden}"
        );
    }
    assert!(js.contains("crypto.subtle") && js.contains("SHA-256"));
}
