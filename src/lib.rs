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
}
