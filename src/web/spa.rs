//! `GET /` — the SPA shell (docs/DESIGN.md §4.1, §6.1).
//!
//! The shell is the Vite build output (`frontend/dist/index.html`), embedded
//! at compile time; the desk renders client-side from the same-origin JSON
//! API. No external request on first paint: fonts, styles, and the bundle
//! all load from `/static/`, and the CSP below is the only policy the page
//! is allowed.

use axum::http::StatusCode;
use axum::response::Response;

use super::html_response;

/// The SPA pages render untrusted counterparty data (reason quotes), so this
/// is a functional requirement, not hardening (docs/DESIGN.md §6.1): scripts,
/// styles, fonts, and the API all come from this origin only.
pub const CSP_SPA: &str = "default-src 'none'; script-src 'self'; style-src 'self'; \
img-src 'self' data:; font-src 'self'; connect-src 'self'; frame-ancestors 'none'; \
base-uri 'none'; form-action 'none'";

const SPA_INDEX: &str = include_str!("../../frontend/dist/index.html");

/// Serves the embedded shell. Short max-age: the bundle and stylesheet are
/// stable-named (`spa.js`/`spa.css`), so the shell must not pin a stale pair.
pub async fn spa_index() -> Response {
    html_response(
        StatusCode::OK,
        CSP_SPA,
        "public, max-age=300",
        SPA_INDEX.to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shell_points_at_the_embedded_assets_and_nothing_external() {
        assert!(SPA_INDEX.contains("src=\"/static/spa.js\""));
        assert!(SPA_INDEX.contains("href=\"/static/spa.css\""));
        assert!(!SPA_INDEX.contains("https://"), "no third-party request");
        assert!(!SPA_INDEX.contains("<script>"), "no inline script");
        assert!(!SPA_INDEX.contains("style=\""), "no inline style");
    }

    #[tokio::test]
    async fn index_answers_with_the_spa_csp() {
        let response = spa_index().await;
        assert_eq!(response.status(), StatusCode::OK);
        let headers = response.headers();
        assert_eq!(headers["content-security-policy"], CSP_SPA);
        assert_eq!(headers["x-content-type-options"], "nosniff");
        assert!(
            headers["cache-control"]
                .to_str()
                .unwrap()
                .contains("max-age=300")
        );
    }
}
