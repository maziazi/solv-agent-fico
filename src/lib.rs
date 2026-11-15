//! Solv — a pre-spend gate and credit bureau for AI agents.
//!
//! The product specification is `docs/SPEC.md`; the work plan is
//! `docs/PLAN.md`; build order and PR rules are `docs/WORKFLOW.md`.

pub mod policy;
pub mod reason;
pub mod score;
pub mod state;
pub mod tier1;

use std::sync::Arc;

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;
use state::Store;

/// Listen port used when `PORT` is unset or empty (README "Development").
pub const DEFAULT_PORT: u16 = 8080;

/// State file used when `STATE_FILE` is unset or empty.
pub const DEFAULT_STATE_FILE: &str = "solv_state.json";

/// Shared application state handed to every handler.
#[derive(Clone)]
pub struct AppState(Arc<Store>);

impl AppState {
    pub fn new(store: Arc<Store>) -> Self {
        Self(store)
    }

    pub fn store(&self) -> &Arc<Store> {
        &self.0
    }
}

/// Builds the application router.
///
/// Kept in the library crate so integration tests can exercise routes
/// in-process without binding a port (AGENTS.md: tests run offline).
pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/api/v1/directory", get(directory))
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
}

/// `GET /api/v1/directory` — every known counterparty, read from live state
/// (SPEC §7.4). Tiers, learned rules, and metrics join as the features
/// behind them land (PR-09, PR-13).
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
    Json(DirectoryResponse { counterparties })
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
        (dir, AppState::new(store))
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
