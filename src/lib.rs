//! Solv — a pre-spend gate and credit bureau for AI agents.
//!
//! The product specification is `docs/SPEC.md`; the work plan is
//! `docs/PLAN.md`; build order and PR rules are `docs/WORKFLOW.md`.

pub mod api;
pub mod bureau;
pub mod metrics;
pub mod policy;
pub mod reason;
pub mod receipt;
pub mod reppo;
pub mod rules;
pub mod score;
pub mod state;
pub mod tier1;
pub mod tier2;
pub mod web;

use std::sync::Arc;

use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Serialize;
use state::Store;
use tier1::ClefClient;
use tier2::ServClient;

/// Listen port used when `PORT` is unset or empty (README "Development").
pub const DEFAULT_PORT: u16 = 8080;

/// State file used when `STATE_FILE` is unset or empty.
pub const DEFAULT_STATE_FILE: &str = "solv_state.json";

/// Shared application state handed to every handler: the state store and
/// the two tier clients behind their traits.
#[derive(Clone)]
pub struct AppState {
    store: Arc<Store>,
    tier1: Arc<dyn ClefClient>,
    tier2: Arc<dyn ServClient>,
}

impl AppState {
    pub fn new(store: Arc<Store>, tier1: Arc<dyn ClefClient>, tier2: Arc<dyn ServClient>) -> Self {
        Self {
            store,
            tier1,
            tier2,
        }
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
    Router::new()
        .route("/healthz", get(healthz))
        .route("/", get(web::spa::spa_index))
        .route("/api/v1/directory", get(directory))
        .route("/api/v1/pre-spend-check", post(api::pre_spend_check))
        .route("/api/v1/receipt/{id}", get(receipt::get_receipt))
        .merge(web::routes())
        .with_state(state)
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

#[derive(Serialize)]
struct DirectoryEntry {
    counterparty_id: String,
    counterparty_name: String,
    target_url: String,
}

#[derive(Serialize)]
struct DirectoryResponse {
    counterparties: Vec<DirectoryEntry>,
    /// SPEC §7.4: metrics computed from stored receipts. A metric without
    /// data is `null`, and the desk renders it as an honest dash.
    metrics: metrics::Metrics,
}

/// `GET /api/v1/directory` — every known counterparty plus bureau metrics,
/// read from live state (SPEC §7.4). Tiers per counterparty, learned rules,
/// and the Reppo quota join as PR-13 lands them.
async fn directory(State(state): State<AppState>) -> Json<DirectoryResponse> {
    let counterparties = state
        .store()
        .list_profiles()
        .into_iter()
        .map(|(counterparty_id, profile)| DirectoryEntry {
            counterparty_id,
            counterparty_name: profile.counterparty_name,
            target_url: profile.target_url,
        })
        .collect();
    let metrics = metrics::compute(&state.store().list_receipts());
    Json(DirectoryResponse {
        counterparties,
        metrics,
    })
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
