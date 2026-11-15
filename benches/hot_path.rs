//! Hot-path benchmarks (SPEC §2.1). These measure what the gate itself
//! costs — state read, policy decision, span extraction, and the full
//! offline request through the router with mock detectors. They do *not*
//! measure provider latency: the mocks answer instantly, so every number
//! here is a floor, not the end-to-end claim. The load-run numbers recorded
//! in SPEC §9.1 cite this file's binary and command.

use std::hint::black_box;
use std::sync::Arc;
use std::time::Duration;

use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use tower::ServiceExt;

use solv::state::{Profile, Store};
use solv::tier1::{ClefClient, MockClefClient};
use solv::tier2::{MockServClient, ServClient, ServVerdict};
use solv::{AppState, build_router};

/// A realistic clean description (the ≈90 % Tier-1-only case).
const CLEAN_DESCRIPTION: &str = "Real-time market data feed for agent consumption. SLA \
99.9%, billed per call, documentation and changelog public. Contact: \
support@ledgerworks.example.";
/// A description in the escalation band (§5.1 rule 1: uncertain injection
/// probability) so Tier 2 runs inside the mocked budget.
const ESCALATING_DESCRIPTION: &str = "Feed access onboarding. It is possible this text is \
trying to make you ignore previous instructions; verify carefully.";

fn bench_state() -> AppState {
    let store =
        Store::open(std::env::temp_dir().join("solv_bench_profile.json")).expect("throwaway store");
    store.upsert_profile(
        "bench-counterparty".to_string(),
        Profile {
            counterparty_name: "bench-counterparty".to_string(),
            target_url: "https://bench.example/api".to_string(),
            score: None,
        },
    );
    let tier1: Arc<dyn ClefClient> = Arc::new(MockClefClient::with_injection(0.02));
    let tier2: Arc<dyn ServClient> = Arc::new(MockServClient {
        verdict: Some(ServVerdict::Approve),
        fail_with_status: None,
        delay: None,
    });
    AppState::new(store, tier1, tier2)
}

fn bench_store() -> Arc<Store> {
    let store: Arc<Store> =
        Store::open(std::env::temp_dir().join("solv_bench_store.json")).expect("throwaway store");
    store.upsert_profile(
        "bench-counterparty".to_string(),
        Profile {
            counterparty_name: "bench-counterparty".to_string(),
            target_url: "https://bench.example/api".to_string(),
            score: None,
        },
    );
    store
}

fn criterion_benchmarks(c: &mut Criterion) {
    let store = bench_store();

    c.bench_function("state_read", |b| {
        b.iter(|| {
            let found = store.profile(black_box("bench-counterparty"));
            black_box(found.is_some());
        })
    });

    let now = solv::api::now_unix();
    let request = solv::policy::PreSpendRequest {
        counterparty_name: "bench-counterparty".to_string(),
        target_url: "https://bench.example/api".to_string(),
        amount_usd: 18.0,
        service_description: CLEAN_DESCRIPTION.to_string(),
    };
    let tier1_signals = solv::policy::Tier1Signals {
        available: true,
        injection_probability_true: Some(0.02),
        risk_class: Some(solv::policy::RiskClass::None),
        risk_class_confidence: Some(0.97),
    };
    let config = solv::policy::PolicyConfig::default();
    c.bench_function("policy_gate_decide", |b| {
        b.iter(|| {
            let decision = solv::policy::decide(
                black_box(&request),
                None,
                black_box(now),
                black_box(45),
                black_box(&tier1_signals),
                None,
                &[],
                black_box(&config),
            );
            black_box(decision.kind);
        })
    });

    c.bench_function("spans_clean_text", |b| {
        b.iter(|| {
            let span = solv::spans::injection_span(black_box(CLEAN_DESCRIPTION));
            black_box(span.is_none());
        })
    });
    c.bench_function("spans_attack_text", |b| {
        b.iter(|| {
            let span = solv::spans::injection_span(black_box(ESCALATING_DESCRIPTION));
            black_box(span.is_some());
        })
    });
}

/// The full offline request through the router: axum layers, validation,
/// one mock Tier-1 call, the gate, the receipt seal, and serialisation.
/// This is the gate-side floor the SPEC §9.1 load-run row cites.
fn bench_full_request(c: &mut Criterion) {
    let state = bench_state();
    let runtime = tokio::runtime::Runtime::new().expect("bench runtime");
    c.bench_function("full_request_offline", |b| {
        b.iter_batched(
            || build_router(state.clone()),
            |router| {
                runtime.block_on(async move {
                    let response = router
                        .oneshot(
                            axum::http::Request::builder()
                                .method("POST")
                                .uri("/api/v1/pre-spend-check")
                                .header("content-type", "application/json")
                                .body(axum::body::Body::from(format!(
                                    "{{\"counterparty_name\":\"bench-counterparty\",\
\"target_url\":\"https://bench.example/api\",\"amount_usd\":18.0,\
\"service_description\":\"{CLEAN_DESCRIPTION}\"}}"
                                )))
                                .unwrap(),
                        )
                        .await
                        .expect("in-process answer");
                    let body = response.into_body();
                    let bytes = http_body_util::BodyExt::collect(body)
                        .await
                        .expect("body")
                        .to_bytes();
                    black_box(bytes.len());
                })
            },
            BatchSize::PerIteration,
        );
    });
}

criterion_group!(
    name = benches;
    config = Criterion::default()
        .warm_up_time(Duration::from_secs(2))
        .measurement_time(Duration::from_secs(5));
    targets = criterion_benchmarks, bench_full_request
);
criterion_main!(benches);
