//! Gate-side load run (SPEC §2.1, §9.1): fires `n` pre-spend checks through
//! the in-process router with mock detectors and reports the decision
//! distribution plus p50/p95/max of each request's WALL time in
//! microseconds — deliberately not the receipt's `latency_ms.total`, which
//! truncates this gate to 0 ms. The measured window includes the router
//! clone, axum dispatch, gate, receipt seal, and JSON parse, so the
//! numbers are a conservative floor for gate-side cost.
//!
//! This measures what the gate itself costs under sequential load. It does
//! not measure provider latency (mocks answer instantly) — the end-to-end
//! budget rows stay honest about that.
//!
//! Usage: `cargo run --release --example load_run -- 1000`

use std::sync::Arc;

use solv::state::Store;
use solv::tier1::{ClefClient, MockClefClient};
use solv::tier2::{MockServClient, ServClient, ServVerdict};
use solv::{AppState, build_router};
use tower::ServiceExt as _;

#[tokio::main]
async fn main() {
    let n: usize = std::env::args()
        .nth(1)
        .and_then(|raw| raw.parse().ok())
        .filter(|&n| n > 0)
        .unwrap_or(1000);

    let store =
        Store::open(std::env::temp_dir().join("solv_load_run.json")).expect("throwaway store");
    let tier1: Arc<dyn ClefClient> = Arc::new(MockClefClient::with_injection(0.02));
    let tier2: Arc<dyn ServClient> = Arc::new(MockServClient {
        verdict: Some(ServVerdict::Approve),
        fail_with_status: None,
        delay: None,
    });
    let state = AppState::new(store, tier1, tier2);

    let body = format!(
        "{{\"counterparty_name\":\"load-run-{n}\",\
\"target_url\":\"https://loadrun.example/api\",\"amount_usd\":18.0,\
\"service_description\":\"Real-time market data feed. SLA 99.9%, billed per \
call, documentation public.\"}}"
    );

    // Per-request wall time in MICROSECONDS: the gate is fast enough that
    // the receipt's millisecond field truncates to 0, which would make the
    // percentiles lie by omission.
    let mut per_request_us: Vec<u128> = Vec::with_capacity(n);
    let mut approvals = 0usize;
    let mut holds = 0usize;
    let started = std::time::Instant::now();
    for _ in 0..n {
        // A fresh router per request: Router is Arc-backed, so the clone is
        // cheap and sits outside the latency the gate itself records.
        let tick = std::time::Instant::now();
        let router = build_router(state.clone());
        let request = axum::http::Request::builder()
            .method("POST")
            .uri("/api/v1/pre-spend-check")
            .header("content-type", "application/json")
            .body(axum::body::Body::from(body.clone()))
            .unwrap();
        let response = router.oneshot(request).await.expect("in-process answer");
        let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024)
            .await
            .expect("body");
        let decision: serde_json::Value = serde_json::from_slice(&bytes).expect("decision");
        match decision["decision"].as_str() {
            Some("APPROVE") => approvals += 1,
            Some("HOLD") => holds += 1,
            _ => {}
        }
        per_request_us.push(tick.elapsed().as_micros());
    }
    let wall = started.elapsed();

    per_request_us.sort_unstable();
    let rank = |p: f64| -> u128 {
        let index = (p / 100.0 * per_request_us.len() as f64).ceil();
        per_request_us[(index as usize).max(1) - 1]
    };

    println!("samples            {n}");
    println!("wall clock         {:.1?}", wall);
    println!(
        "decisions          {approvals} APPROVE / {holds} HOLD / {} total \
(unrated clean path -> HOLD is the expected outcome)",
        per_request_us.len()
    );
    println!("gate p50 / p95     {}µs / {}µs", rank(50.0), rank(95.0));
    println!(
        "gate p100          {}µs",
        per_request_us.last().copied().unwrap_or(0)
    );
}
