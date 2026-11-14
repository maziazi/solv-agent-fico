//! Solv — a pre-spend gate and credit bureau for AI agents.
//!
//! The product specification is `docs/SPEC.md`; the work plan is
//! `docs/PLAN.md`; build order and PR rules are `docs/WORKFLOW.md`.

use axum::Router;
use axum::routing::get;

/// Builds the application router.
///
/// Kept in the library crate so integration tests can exercise routes
/// in-process without binding a port (AGENTS.md: tests run offline).
pub fn build_router() -> Router {
    Router::new().route("/healthz", get(healthz))
}

async fn healthz() -> &'static str {
    "ok"
}

/// Listen port used when `PORT` is unset or empty (README "Development").
pub const DEFAULT_PORT: u16 = 8080;

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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    #[tokio::test]
    async fn healthz_answers_200_ok() {
        let response = build_router()
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
        let response = build_router()
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
}
