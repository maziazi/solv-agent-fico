//! Solv — a pre-spend gate and credit bureau for AI agents.
//!
//! The product specification is `docs/SPEC.md`; the work plan is
//! `docs/PLAN.md`; build order and PR rules are `docs/WORKFLOW.md`.

pub mod api;
pub mod bureau;
pub mod directory;
pub mod metrics;
pub mod policy;
pub mod reason;
pub mod receipt;
pub mod reppo;
pub mod rules;
pub mod score;
pub mod spans;
pub mod state;
pub mod tier1;
pub mod tier2;
pub mod web;

use std::sync::Arc;

use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use state::Store;
use tier1::ClefClient;
use tier2::ServClient;

/// Listen port used when `PORT` is unset or empty (README "Development").
pub const DEFAULT_PORT: u16 = 8080;

/// State file used when `STATE_FILE` is unset or empty.
pub const DEFAULT_STATE_FILE: &str = "solv_state.json";

/// Shared application state handed to every handler: the state store, the
/// two tier clients behind their traits, and — when the operator has
/// enabled and budgeted Reppo — the audit worker that owns the single
/// evaluation slot.
#[derive(Clone)]
pub struct AppState {
    store: Arc<Store>,
    tier1: Arc<dyn ClefClient>,
    tier2: Arc<dyn ServClient>,
    bureau: Option<Arc<bureau::Bureau>>,
    /// Resolved once at boot: SPEC §2.1 defaults unless the operator set
    /// `SOLV_LATENCY_CEILING_MS` for a slower route.
    policy_config: policy::PolicyConfig,
}

impl AppState {
    pub fn new(store: Arc<Store>, tier1: Arc<dyn ClefClient>, tier2: Arc<dyn ServClient>) -> Self {
        Self {
            store,
            tier1,
            tier2,
            bureau: None,
            policy_config: policy::PolicyConfig::default(),
        }
    }

    /// Overrides the policy configuration (currently only the §2.1 latency
    /// ceiling is operator-settable).
    pub fn with_policy_config(mut self, policy_config: policy::PolicyConfig) -> Self {
        self.policy_config = policy_config;
        self
    }

    /// Attaches the audit worker. `None` (the default) is the honest
    /// fail-closed state: enqueueing is impossible and the directory says
    /// the worker is not attached, rather than showing a zero meter.
    pub fn with_bureau(mut self, bureau: Option<Arc<bureau::Bureau>>) -> Self {
        self.bureau = bureau;
        self
    }

    pub fn bureau(&self) -> Option<&Arc<bureau::Bureau>> {
        self.bureau.as_ref()
    }

    pub fn policy_config(&self) -> &policy::PolicyConfig {
        &self.policy_config
    }

    pub fn store(&self) -> &Arc<Store> {
        &self.store
    }

    pub fn tier1(&self) -> &Arc<dyn ClefClient> {
        &self.tier1
    }

    pub fn tier2(&self) -> &Arc<dyn ServClient> {
        &self.tier2
    }
}

/// Builds the application router.
///
/// Kept in the library crate so integration tests can exercise routes
/// in-process without binding a port (AGENTS.md: tests run offline).
pub fn build_router(state: AppState) -> Router {
    // Every frontend route serves the SPA shell; the client router resolves
    // the path (docs/FRONTEND-V2.md §2). `/bureau` is kept as a deep link —
    // the shell redirects it to `/explorer` client-side, so existing links
    // survive the rename.
    const SPA_ROUTES: &[&str] = &[
        "/",
        "/desk",
        "/explorer",
        "/bureau",
        "/how-it-works",
        "/docs",
        "/pricing",
        "/attack-lab",
    ];
    let api = Router::new()
        .route("/healthz", get(healthz))
        .route("/api/v1/directory", get(directory))
        .route("/api/v1/pre-spend-check", post(api::pre_spend_check))
        .route("/api/v1/audit-counterparty", post(api::audit_counterparty))
        .route("/api/v1/receipt/{id}", get(receipt::get_receipt));
    let mut app = api.merge(web::routes());
    for path in SPA_ROUTES {
        app = app.route(path, get(web::spa::spa_index));
    }
    app.with_state(state)
}

async fn healthz() -> &'static str {
    "ok"
}

/// Serves the router until `shutdown` resolves, then flushes a final
/// snapshot before returning.
///
/// The explicit flush closes the debounce window: without it a SIGTERM
/// (every deploy) would drop up to 2 seconds of acknowledged state.
pub async fn run_server(
    listener: tokio::net::TcpListener,
    state: AppState,
    shutdown: impl std::future::Future<Output = ()> + Send + 'static,
) -> std::io::Result<()> {
    let store = Arc::clone(state.store());
    axum::serve(listener, build_router(state))
        .with_graceful_shutdown(shutdown)
        .await?;
    if let Err(error) = store.write_snapshot_now() {
        eprintln!("solv: final snapshot write failed: {error}");
    }
    Ok(())
}

/// `GET /api/v1/directory` — every counterparty with its tier, the learned
/// rules (active and shadow), the quota state, and real metrics, read from
/// live state (SPEC §7.4). Assembly and its honesty rules live in
/// `src/directory.rs`; this handler only gathers the store snapshots.
async fn directory(State(state): State<AppState>) -> Json<directory::DirectoryPayload> {
    let store = state.store();
    let reppo = match state.bureau() {
        Some(worker) => directory::ReppoQuota::attached(worker.credits_remaining()),
        None => directory::ReppoQuota::not_attached(),
    };
    let payload = directory::build(
        api::now_unix(),
        &store.list_profiles(),
        &store.list_receipts(),
        &store.learned_rules(),
        reppo,
    );
    Json(payload)
}

/// Resolves the listen port from the `PORT` environment variable.
///
/// Unset or empty falls back to [`DEFAULT_PORT`]. A set-but-invalid value is
/// an operator error and returns `Err` so the process can fail loudly instead
/// of silently binding the default port.
pub fn resolve_port(var: Option<&str>) -> Result<u16, String> {
    match var {
        None | Some("") => Ok(DEFAULT_PORT),
        Some(raw) => raw
            .parse::<u16>()
            .map_err(|_| format!("invalid PORT value: {raw:?}")),
    }
}

/// Resolves the state file path from the `STATE_FILE` environment variable.
pub fn resolve_state_file(var: Option<&str>) -> std::path::PathBuf {
    match var {
        None | Some("") => std::path::PathBuf::from(DEFAULT_STATE_FILE),
        Some(raw) => std::path::PathBuf::from(raw),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    /// A router state backed by a throwaway store. Keep the returned
    /// `TempDir` alive for the test's duration.
    fn test_state() -> (tempfile::TempDir, AppState) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join("state.json")).unwrap();
        let tier1: std::sync::Arc<dyn ClefClient> =
            std::sync::Arc::new(tier1::MockClefClient::default());
        let tier2: std::sync::Arc<dyn ServClient> =
            std::sync::Arc::new(tier2::MockServClient::default());
        (dir, AppState::new(store, tier1, tier2))
    }

    #[tokio::test]
    async fn healthz_answers_200_ok() {
        let response = build_router(test_state().1)
            .oneshot(
                Request::builder()
                    .uri("/healthz")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(&body[..], b"ok");
    }

    #[tokio::test]
    async fn unknown_route_answers_404() {
        let response = build_router(test_state().1)
            .oneshot(
                Request::builder()
                    .uri("/definitely-not-a-route")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn directory_lists_profiles_from_state() {
        let (_dir, state) = test_state();
        state.store().upsert_profile(
            "cp-b".to_string(),
            state::Profile {
                counterparty_name: "DataFeed Pro".to_string(),
                target_url: "https://api.example.com/v1/feed".to_string(),
                score: None,
            },
        );
        state.store().upsert_profile(
            "cp-a".to_string(),
            state::Profile {
                counterparty_name: "Translate Bot".to_string(),
                target_url: "https://translate.example.com".to_string(),
                score: None,
            },
        );

        let response = build_router(state)
            .oneshot(
                Request::builder()
                    .uri("/api/v1/directory")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let ids: Vec<&str> = json["counterparties"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| entry["counterparty_id"].as_str().unwrap())
            .collect();
        assert_eq!(ids, vec!["cp-a", "cp-b"], "sorted and complete");
    }

    #[tokio::test]
    async fn index_serves_the_spa_shell_under_the_strict_policy() {
        let response = build_router(test_state().1)
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let csp = response
            .headers()
            .get("content-security-policy")
            .expect("CSP header")
            .to_str()
            .unwrap()
            .to_string();
        assert!(csp.contains("script-src 'self'"), "csp: {csp}");
        assert!(csp.contains("connect-src 'self'"), "csp: {csp}");
        assert!(!csp.contains("unsafe"), "csp: {csp}");
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let body = String::from_utf8(body.to_vec()).unwrap();
        assert!(body.contains("src=\"/static/spa.js\""), "shell: {body}");
        // No third-party fetch: only local asset references. (The favicon is
        // a data: URI whose XML namespace happens to contain the letters
        // "http" — that is not a request, so the check is on fetch sites.)
        for needle in [
            "src=\"http",
            "href=\"http",
            "url(http",
            "@import",
            "fetch(\"http",
        ] {
            assert!(
                !body.contains(needle),
                "external fetch site {needle} in shell"
            );
        }
    }

    #[tokio::test]
    async fn directory_metrics_are_computed_from_real_receipts() {
        let (_dir, state) = test_state();
        let now = api::now_unix();
        let response_json = serde_json::json!({
            "tier": "HIGH_RISK",
            "tier_used": ["clef-flash", "serv-reasoning"],
            "latency_ms": { "state": 0, "tier1": 40, "tier2": 600, "total": 660 }
        });
        assert!(state.store().insert_receipt(
            "slv_test_block_1".to_string(),
            state::Receipt {
                decision: "BLOCK".to_string(),
                amount_usd: 50.0,
                created_at_unix: now,
                evidence_sha256: "ab".to_string(),
                evidence: serde_json::json!({}),
                response: response_json,
            },
        ));
        assert!(state.store().insert_receipt(
            "slv_test_approve_1".to_string(),
            state::Receipt {
                decision: "APPROVE".to_string(),
                amount_usd: 18.0,
                created_at_unix: now,
                evidence_sha256: "cd".to_string(),
                evidence: serde_json::json!({}),
                response: serde_json::json!({
                    "tier": "PRIME",
                    "tier_used": ["clef-flash"],
                    "latency_ms": { "state": 0, "tier1": 44, "total": 45 }
                }),
            },
        ));

        let response = build_router(state)
            .oneshot(
                Request::builder()
                    .uri("/api/v1/directory")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let metrics = &json["metrics"];
        assert_eq!(metrics["receipts_total"], 2);
        assert_eq!(metrics["decisions"]["block"], 1);
        assert_eq!(metrics["decisions"]["approve"], 1);
        assert_eq!(metrics["total_blocked_usd"], 50.0);
        assert_eq!(metrics["latency_ms"]["tier1_only_p50"], 45);
        assert_eq!(metrics["latency_ms"]["escalated_p50"], 660);
        assert_eq!(
            metrics["cost_per_check_usd"],
            serde_json::Value::Null,
            "no token counts, no cost number"
        );
    }

    #[tokio::test]
    async fn directory_enriches_counterparties_rules_and_quota() {
        let (_dir, state) = test_state();
        state.store().upsert_profile(
            "cp-a".to_string(),
            state::Profile {
                counterparty_name: "Cold Start Ltd".to_string(),
                target_url: "https://cold.example/api".to_string(),
                score: None,
            },
        );
        assert!(state.store().insert_receipt(
            "slv_test_h_1".to_string(),
            state::Receipt {
                decision: "HOLD".to_string(),
                amount_usd: 5.0,
                created_at_unix: api::now_unix(),
                evidence_sha256: "ab".to_string(),
                evidence: serde_json::json!({ "request": { "counterparty_id": "cp-a" } }),
                response: serde_json::json!({}),
            },
        ));
        state.store().set_learned_rules(vec![state::LearnedRule {
            rule_id: "SOLV-RULE-001".to_string(),
            status: state::RuleStatus::Shadow,
            topic: Some(rules::RuleTopic::GuaranteedRefund),
            effect: policy::RuleEffect::ForcedHold,
            threshold: rules::DEFAULT_THRESHOLD,
            created_from: Some(rules::RuleSource {
                eval_id: "ev_9".to_string(),
                criterion: "guaranteed refund".to_string(),
                critique_quote: "promises a refund no process can grant".to_string(),
            }),
        }]);

        let response = build_router(state)
            .oneshot(
                Request::builder()
                    .uri("/api/v1/directory")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        let cp = &json["counterparties"][0];
        assert_eq!(cp["counterparty_id"], "cp-a");
        assert_eq!(cp["tier"], "UNRATED");
        assert_eq!(cp["last_decision"]["decision"], "HOLD");
        assert_eq!(cp["last_decision"]["receipt_id"], "slv_test_h_1");

        let rule = &json["learned_rules"][0];
        assert_eq!(rule["rule_id"], "SOLV-RULE-001");
        assert_eq!(rule["status"], "shadow");
        assert_eq!(rule["topic"], "guaranteed_refund");
        assert_eq!(
            rule["created_from"]["critique_quote"],
            "promises a refund no process can grant"
        );

        let reppo = &json["reppo"];
        assert_eq!(reppo["worker_attached"], false);
        assert_eq!(reppo["credits_remaining"], serde_json::Value::Null);
        assert_eq!(json["metrics"]["receipts_total"], 1);
    }

    /// A worker over the scripted mock, with a live credit budget, plus
    /// its temp store. The mock never reaches the network.
    fn test_state_with_worker(credits: u32) -> (tempfile::TempDir, AppState) {
        use crate::bureau::{Bureau, BureauConfig};
        use crate::reppo::{MockOutcome, MockReppoClient};
        let (dir, state) = test_state();
        // One scripted settled evaluation. The enqueue leg never touches
        // the client — the worker is aborted before it can accept a job —
        // so the outcome is spare rather than consumed by these tests.
        let client: Arc<dyn reppo::ReppoClient> = Arc::new(MockReppoClient::new(
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
        let (worker, handle) = Bureau::spawn(client, state.store().clone(), config);
        handle.abort(); // the enqueue path never needs the polling loop
        (dir, state.with_bureau(Some(Arc::new(worker))))
    }

    #[tokio::test]
    async fn audit_endpoint_queues_and_returns_the_202_contract() {
        let (_dir, state) = test_state_with_worker(500);
        let response = build_router(state)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/audit-counterparty")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "counterparty_id": "datafeed-pro",
                            "target_url": "https://api.example.com/v1/feed",
                            "claim": "Real-time market data, 1 month access."
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::ACCEPTED);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "QUEUED");
        assert_eq!(json["queue_position"], 1);
        assert_eq!(json["settlement_minutes"], 10);
        assert!(json["estimated_settle_at"].as_str().is_some());
        assert_eq!(json["credits_remaining"], 500);
        assert!(json["note"].as_str().unwrap().contains("10 minutes"));
    }

    #[tokio::test]
    async fn audit_endpoint_refuses_when_the_credit_budget_is_spent() {
        let (_dir, state) = test_state_with_worker(0);
        let response = build_router(state)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/audit-counterparty")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "counterparty_id": "someone",
                            "target_url": "https://someone.example/api",
                            "claim": "A claim."
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::PAYMENT_REQUIRED);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["error"].as_str().unwrap().contains("credit"));
    }

    #[tokio::test]
    async fn audit_endpoint_answers_503_when_the_worker_is_not_attached() {
        let (_dir, state) = test_state();
        let response = build_router(state)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/audit-counterparty")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "counterparty_id": "someone",
                            "target_url": "https://someone.example/api",
                            "claim": "A claim."
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn directory_reports_the_live_worker_and_credit_counter() {
        let (_dir, state) = test_state_with_worker(500);
        let response = build_router(state)
            .oneshot(
                Request::builder()
                    .uri("/api/v1/directory")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["reppo"]["worker_attached"], true);
        assert_eq!(json["reppo"]["credits_remaining"], 500);
    }

    #[tokio::test]
    async fn bureau_serves_the_spa_shell_for_deep_links() {
        let response = build_router(test_state().1)
            .oneshot(
                Request::builder()
                    .uri("/bureau")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let csp = response
            .headers()
            .get("content-security-policy")
            .expect("CSP header")
            .to_str()
            .unwrap()
            .to_string();
        assert!(csp.contains("script-src 'self'"), "csp: {csp}");
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let body = String::from_utf8(body.to_vec()).unwrap();
        assert!(body.contains("src=\"/static/spa.js\""), "shell: {body}");
    }

    #[tokio::test]
    async fn empty_directory_metrics_report_no_data_not_zeros_dressed_up() {
        let (_dir, state) = test_state();
        let response = build_router(state)
            .oneshot(
                Request::builder()
                    .uri("/api/v1/directory")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let metrics = &json["metrics"];
        assert_eq!(metrics["receipts_total"], 0);
        assert_eq!(
            metrics["latency_ms"]["tier1_only_p50"],
            serde_json::Value::Null
        );
    }

    #[test]
    fn resolve_port_defaults_when_unset_or_empty() {
        assert_eq!(resolve_port(None).unwrap(), DEFAULT_PORT);
        assert_eq!(resolve_port(Some("")).unwrap(), DEFAULT_PORT);
    }

    #[test]
    fn resolve_port_parses_a_valid_value() {
        assert_eq!(resolve_port(Some("9000")).unwrap(), 9000);
    }

    #[test]
    fn resolve_port_rejects_invalid_values() {
        assert!(resolve_port(Some("abc")).is_err());
        assert!(resolve_port(Some("99999")).is_err());
        assert!(resolve_port(Some("0x10")).is_err());
    }

    #[test]
    fn resolve_state_file_defaults_when_unset_or_empty() {
        assert_eq!(
            resolve_state_file(None),
            std::path::PathBuf::from(DEFAULT_STATE_FILE)
        );
        assert_eq!(
            resolve_state_file(Some("")),
            std::path::PathBuf::from(DEFAULT_STATE_FILE)
        );
        assert_eq!(
            resolve_state_file(Some("/tmp/custom.json")),
            std::path::PathBuf::from("/tmp/custom.json")
        );
    }
}
