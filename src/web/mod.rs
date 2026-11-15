//! Server-rendered web pages and their shared shell (docs/DESIGN.md §4, §6).
//!
//! Layout contract for the pages that follow (underwriting desk, bureau
//! directory):
//!
//! - `templates/layout.html` is the one page shell. [`layout::page`] fills
//!   its `{{title}}` and `{{content}}` slots; a page supplies only its
//!   `<main>` content.
//! - `static/app.css` is the one stylesheet, served at `/static/app.css`
//!   from memory. Page-specific rules go into the same file under a
//!   commented section, so there is still exactly one request for styles.
//! - No external request is ever made on first paint: no CDN, no web fonts
//!   (docs/DESIGN.md §6).
//! - Every response carries a Content-Security-Policy. [`CSP_NO_SCRIPT`]
//!   is the default and forbids scripts outright; a page that needs
//!   JavaScript (PR-11) must add its own policy constant with
//!   `script-src 'self'` and serve the script as a file. `unsafe-inline`
//!   is never an option.
//! - Dynamic values are escaped with [`escape::escape`] and placed only in
//!   element text.

pub mod escape;
pub mod layout;
pub mod receipt_page;

use axum::Router;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;

use crate::AppState;

/// Strict policy for pages without JavaScript: scripts, frames, forms, and
/// every fetch are denied; only same-origin styles and inline `data:`
/// images load.
pub const CSP_NO_SCRIPT: &str = "default-src 'none'; style-src 'self'; img-src 'self' data:; \
font-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'";

const APP_CSS: &str = include_str!("../../static/app.css");

/// Routes of the web pages. Merged into the application router.
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/receipt/{id}", get(receipt_page::receipt_page))
        .route("/static/app.css", get(app_css))
}

/// An HTML response with the hardened header set every page shares.
pub fn html_response(
    status: StatusCode,
    csp: &'static str,
    cache: &'static str,
    body: String,
) -> Response {
    (
        status,
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
            (header::CONTENT_SECURITY_POLICY, csp),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
            (header::REFERRER_POLICY, "no-referrer"),
            (header::X_FRAME_OPTIONS, "DENY"),
            (header::CACHE_CONTROL, cache),
        ],
        body,
    )
        .into_response()
}

async fn app_css() -> Response {
    (
        [
            (header::CONTENT_TYPE, "text/css; charset=utf-8"),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
            (header::CACHE_CONTROL, "public, max-age=3600"),
            (header::CONTENT_SECURITY_POLICY, "default-src 'none'"),
        ],
        APP_CSS,
    )
        .into_response()
}
