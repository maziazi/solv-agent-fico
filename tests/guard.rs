//! Public-deploy guard tests (docs/SPEC.md §8.2), all offline: the per-client
//! rate limit and concurrency cap on `pre-spend-check`, where the client
//! address comes from (a spoofed header must not buy a fresh allowance), and
//! the closed-by-default admin token on `audit-counterparty`.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use solv::bureau::{Bureau, BureauConfig};
use solv::guard::LimitsConfig;
use solv::reppo::{MockOutcome, MockReppoClient, ReppoClient};
use solv::state::Store;
use solv::tier1::{ClefClient, MockClefClient};
use solv::tier2::{MockServClient, ServClient};
use solv::{AppState, build_router};
use tokio::sync::Semaphore;
use tower::ServiceExt;

const ADMIN_TOKEN: &str = "integration-admin-token-4242";

struct Harness {
    router: axum::Router,
    tier2_started: Arc<Semaphore>,
    _dir: tempfile::TempDir,
}

fn harness(
    limits: Option<LimitsConfig>,
    admin: Option<&str>,
    tier2_delay: Option<Duration>,
) -> Harness {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("state.json")).unwrap();
    let tier1: Arc<dyn ClefClient> = Arc::new(MockClefClient::with_injection(0.02));
    let tier2_started = Arc::new(Semaphore::new(0));
    // An UNRATED counterparty always escalates, so the delay holds the
    // request (and its concurrency permit) open.
    let tier2: Arc<dyn ServClient> = Arc::new(MockServClient {
        delay: tier2_delay,
        started: Some(Arc::clone(&tier2_started)),
        ..MockServClient::default()
    });
    let mut state = AppState::new(store, tier1, tier2).with_admin_token(admin);
    if let Some(limits) = limits {
        state = state.with_limits(limits);
    }
    Harness {
        router: build_router(state),
        tier2_started,
        _dir: dir,
    }
}

fn with_worker(credits: u32, admin: Option<&str>) -> Harness {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("state.json")).unwrap();
    let tier1: Arc<dyn ClefClient> = Arc::new(MockClefClient::with_injection(0.02));
    let tier2: Arc<dyn ServClient> = Arc::new(MockServClient::default());
    let client: Arc<dyn ReppoClient> = Arc::new(MockReppoClient::new(
        vec![MockOutcome::Settled {
            reppo_score: 84.0,
            pillars: [8, 8, 8, 8],
        }],
        0,
    ));
    let config = BureauConfig {
        starting_credits: credits,
        ..BureauConfig::default()
    };
    let (worker, handle) = Bureau::spawn(client, Arc::clone(&store), config);
    handle.abort();
    let state = AppState::new(store, tier1, tier2)
        .with_bureau(Some(Arc::new(worker)))
        .with_admin_token(admin);
    Harness {
        router: build_router(state),
        tier2_started: Arc::new(Semaphore::new(0)),
        _dir: dir,
    }
}

fn peer(ip: &str) -> SocketAddr {
    format!("{ip}:40000").parse().unwrap()
}

fn check_request(from: SocketAddr, forwarded: Option<&str>) -> Request<Body> {
    check_request_with_description(from, forwarded, "A plain description of a plain service.")
}

fn check_request_with_description(
    from: SocketAddr,
    forwarded: Option<&str>,
    service_description: &str,
) -> Request<Body> {
    let mut builder = Request::builder()
        .method("POST")
        .uri("/api/v1/pre-spend-check")
        .header("content-type", "application/json");
    if let Some(forwarded) = forwarded {
        builder = builder.header("x-forwarded-for", forwarded);
    }
    let mut request = builder
        .body(Body::from(
            json!({
                "counterparty_name": "nobody-yet",
                "target_url": "https://nobody.example/api",
                "amount_usd": 5,
                "service_description": service_description
            })
            .to_string(),
        ))
        .unwrap();
    request.extensions_mut().insert(ConnectInfo(from));
    request
}

async fn send(
    router: &axum::Router,
    request: Request<Body>,
) -> (StatusCode, axum::http::HeaderMap, Vec<u8>) {
    let response = router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let body = response
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes()
        .to_vec();
    (status, headers, body)
}

fn audit_request(authorization: Option<&str>, body: Body) -> Request<Body> {
    let mut builder = Request::builder()
        .method("POST")
        .uri("/api/v1/audit-counterparty")
        .header("content-type", "application/json");
    if let Some(value) = authorization {
        builder = builder.header("authorization", value);
    }
    builder.body(body).unwrap()
}

fn audit_body() -> Body {
    Body::from(
        json!({
            "counterparty_id": "someone",
            "target_url": "https://someone.example/api",
            "claim": "A claim."
        })
        .to_string(),
    )
}

// ---------------------------------------------------------------------------
// Rate limit
// ---------------------------------------------------------------------------

#[tokio::test]
async fn at_default_settings_the_21st_request_inside_a_minute_is_refused() {
    let h = harness(Some(LimitsConfig::default()), None, None);
    let client = peer("203.0.113.10");
    let mut refused_from = None;
    // 200 sequential requests: the first 20 are served, every later one is
    // refused.
    for n in 1..=200 {
        let (status, _, _) = send(&h.router, check_request(client, None)).await;
        match status {
            StatusCode::OK => assert!(refused_from.is_none(), "served after a refusal at {n}"),
            StatusCode::TOO_MANY_REQUESTS => {
                refused_from.get_or_insert(n);
            }
            other => panic!("request {n}: unexpected {other}"),
        }
    }
    assert_eq!(refused_from, Some(21));
}

#[tokio::test]
async fn a_refusal_is_a_429_with_retry_after_and_a_json_error() {
    let h = harness(
        Some(LimitsConfig {
            per_minute: 1,
            ..LimitsConfig::default()
        }),
        None,
        None,
    );
    let client = peer("203.0.113.11");
    assert_eq!(
        send(&h.router, check_request(client, None)).await.0,
        StatusCode::OK
    );

    let (status, headers, body) = send(&h.router, check_request(client, None)).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    let retry: u64 = headers["retry-after"].to_str().unwrap().parse().unwrap();
    assert!((1..=60).contains(&retry), "{retry}");
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["error"], "rate limit exceeded");
    assert_eq!(json["retry_after_seconds"], retry);
}

#[tokio::test]
async fn clients_have_separate_allowances() {
    let h = harness(
        Some(LimitsConfig {
            per_minute: 2,
            ..LimitsConfig::default()
        }),
        None,
        None,
    );
    let (a, b) = (peer("203.0.113.20"), peer("203.0.113.21"));
    for _ in 0..2 {
        assert_eq!(
            send(&h.router, check_request(a, None)).await.0,
            StatusCode::OK
        );
    }
    assert_eq!(
        send(&h.router, check_request(a, None)).await.0,
        StatusCode::TOO_MANY_REQUESTS
    );
    assert_eq!(
        send(&h.router, check_request(b, None)).await.0,
        StatusCode::OK
    );
}

#[tokio::test]
async fn a_spoofed_forwarded_header_does_not_buy_a_fresh_allowance() {
    // Default: the proxy is not trusted, so the header means nothing.
    let h = harness(
        Some(LimitsConfig {
            per_minute: 3,
            ..LimitsConfig::default()
        }),
        None,
        None,
    );
    let client = peer("203.0.113.30");
    let mut served = 0;
    for n in 0..30 {
        let forged = format!("198.51.100.{n}");
        let (status, _, _) = send(&h.router, check_request(client, Some(&forged))).await;
        if status == StatusCode::OK {
            served += 1;
        }
    }
    assert_eq!(
        served, 3,
        "rotating X-Forwarded-For must not reset the limit"
    );
}

#[tokio::test]
async fn behind_a_trusted_proxy_the_rightmost_entry_is_the_client() {
    let h = harness(
        Some(LimitsConfig {
            per_minute: 2,
            trust_proxy: true,
            ..LimitsConfig::default()
        }),
        None,
        None,
    );
    let proxy = peer("10.0.0.1");

    // Two real clients behind one proxy keep separate allowances.
    for _ in 0..2 {
        assert_eq!(
            send(&h.router, check_request(proxy, Some("203.0.113.40")))
                .await
                .0,
            StatusCode::OK
        );
    }
    assert_eq!(
        send(&h.router, check_request(proxy, Some("203.0.113.40")))
            .await
            .0,
        StatusCode::TOO_MANY_REQUESTS
    );
    assert_eq!(
        send(&h.router, check_request(proxy, Some("203.0.113.41")))
            .await
            .0,
        StatusCode::OK
    );

    // A caller prepending forged entries to the left cannot escape: the
    // proxy still appends its real address last.
    for n in 0..10 {
        let chain = format!("192.0.2.{n}, 203.0.113.40");
        assert_eq!(
            send(&h.router, check_request(proxy, Some(&chain))).await.0,
            StatusCode::TOO_MANY_REQUESTS,
            "forged left entry {n}"
        );
    }
}

#[tokio::test]
async fn public_read_routes_are_not_limited() {
    let h = harness(
        Some(LimitsConfig {
            per_minute: 1,
            max_concurrent: 1,
            ..LimitsConfig::default()
        }),
        None,
        None,
    );
    for uri in ["/healthz", "/api/v1/directory", "/", "/explorer"] {
        for _ in 0..50 {
            let mut request = Request::builder().uri(uri).body(Body::empty()).unwrap();
            request
                .extensions_mut()
                .insert(ConnectInfo(peer("203.0.113.50")));
            assert_eq!(send(&h.router, request).await.0, StatusCode::OK, "{uri}");
        }
    }
}

// ---------------------------------------------------------------------------
// Concurrency cap
// ---------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn the_concurrency_cap_serves_n_and_refuses_the_rest_then_recovers() {
    let h = harness(
        Some(LimitsConfig {
            per_minute: 1_000,
            max_concurrent: 2,
            ..LimitsConfig::default()
        }),
        None,
        Some(Duration::from_millis(300)),
    );

    let tasks: Vec<_> = (0..5)
        .map(|n| {
            let router = h.router.clone();
            // Distinct clients, so only the global cap can refuse.
            let from = peer(&format!("203.0.113.{}", 60 + n));
            tokio::spawn(async move {
                send(
                    &router,
                    check_request_with_description(from, None, &"x".repeat(1_501)),
                )
                .await
            })
        })
        .collect();
    let mut served = 0;
    let mut refused = 0;
    h.tier2_started.acquire_many(2).await.unwrap().forget();
    for task in tasks {
        let (status, headers, body) = task.await.unwrap();
        match status {
            StatusCode::OK => served += 1,
            StatusCode::TOO_MANY_REQUESTS => {
                refused += 1;
                assert!(headers.contains_key("retry-after"));
                let json: Value = serde_json::from_slice(&body).unwrap();
                assert_eq!(json["error"], "too many checks in flight");
            }
            other => panic!("unexpected {other}"),
        }
    }
    assert_eq!((served, refused), (2, 3));

    // Permits come back when the checks finish.
    let (status, _, _) = send(&h.router, check_request(peer("203.0.113.70"), None)).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test(start_paused = true)]
async fn a_load_refusal_does_not_spend_the_clients_own_allowance() {
    let h = harness(
        Some(LimitsConfig {
            per_minute: 1,
            max_concurrent: 1,
            ..LimitsConfig::default()
        }),
        None,
        Some(Duration::from_millis(300)),
    );
    let busy = {
        let router = h.router.clone();
        tokio::spawn(async move {
            send(
                &router,
                check_request_with_description(peer("203.0.113.80"), None, &"x".repeat(1_501)),
            )
            .await
        })
    };
    h.tier2_started.acquire().await.unwrap().forget();
    // The cap is full, so this one is refused for load...
    let victim = peer("203.0.113.81");
    let (status, _, _) = send(&h.router, check_request(victim, None)).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(busy.await.unwrap().0, StatusCode::OK);
    // ...and the client can still use its single request once load drops.
    let (status, _, _) = send(&h.router, check_request(victim, None)).await;
    assert_eq!(status, StatusCode::OK);
}

// ---------------------------------------------------------------------------
// Admin token
// ---------------------------------------------------------------------------

#[tokio::test]
async fn audit_with_the_right_token_reaches_the_handler() {
    let h = with_worker(500, Some(ADMIN_TOKEN));
    let bearer = format!("Bearer {ADMIN_TOKEN}");
    let (status, _, body) = send(&h.router, audit_request(Some(&bearer), audit_body())).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "QUEUED");
}

#[tokio::test]
async fn audit_refuses_every_wrong_credential_with_the_same_401() {
    let h = with_worker(500, Some(ADMIN_TOKEN));
    let wrong_len = format!("Bearer {ADMIN_TOKEN}x");
    let near = format!("Bearer {}", &ADMIN_TOKEN[..ADMIN_TOKEN.len() - 1]);
    let basic = format!("Basic {ADMIN_TOKEN}");
    let attempts: Vec<Option<&str>> = vec![
        None,
        Some(""),
        Some("Bearer"),
        Some("Bearer "),
        Some("Bearer wrong"),
        Some(&wrong_len),
        Some(&near),
        Some(&basic),
        Some(ADMIN_TOKEN),
    ];
    let mut bodies = Vec::new();
    for attempt in attempts {
        let (status, headers, body) = send(&h.router, audit_request(attempt, audit_body())).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{attempt:?}");
        assert_eq!(headers["www-authenticate"], "Bearer");
        bodies.push(body);
    }
    assert!(
        bodies.windows(2).all(|pair| pair[0] == pair[1]),
        "the answer must not say which part was wrong"
    );
}

#[tokio::test]
async fn audit_is_closed_when_no_token_is_configured() {
    for configured in [None, Some("")] {
        let h = with_worker(500, configured);
        for authorization in [
            None,
            Some("Bearer "),
            Some("Bearer anything"),
            Some("Bearer "),
        ] {
            let (status, _, _) = send(&h.router, audit_request(authorization, audit_body())).await;
            assert_eq!(
                status,
                StatusCode::UNAUTHORIZED,
                "{configured:?} {authorization:?}"
            );
        }
    }
}

#[tokio::test]
async fn an_unauthenticated_caller_learns_nothing_about_the_worker_or_the_body() {
    // Worker not attached AND no token: still 401, not the 503 that would
    // reveal whether queueing is possible.
    let h = harness(None, None, None);
    let (status, _, _) = send(&h.router, audit_request(None, audit_body())).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // A malformed body is not parsed before authentication (no 422).
    let h = with_worker(500, Some(ADMIN_TOKEN));
    let (status, _, _) = send(&h.router, audit_request(None, Body::from("{not json"))).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // Authenticated, the honest 503 comes back.
    let h = harness(None, Some(ADMIN_TOKEN), None);
    let bearer = format!("Bearer {ADMIN_TOKEN}");
    let (status, _, _) = send(&h.router, audit_request(Some(&bearer), audit_body())).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn the_admin_token_does_not_unlock_anything_else_and_checks_need_none() {
    let h = harness(Some(LimitsConfig::default()), Some(ADMIN_TOKEN), None);
    // pre-spend-check never asks for a token.
    let (status, _, _) = send(&h.router, check_request(peer("203.0.113.90"), None)).await;
    assert_eq!(status, StatusCode::OK);
}
