//! Server-rendered web pages and their shared shell (docs/DESIGN.md §4, §6).
//!
//! Layout contract for the pages that follow (underwriting desk, bureau
//! directory):
//!
//! - `templates/layout.html` is the one page shell. A page builds a
//!   [`layout::Page`] and calls [`layout::render`]. Slots: `title` (plain
//!   text, escaped by the shell), `content`, `footer`, `head_extra` and
//!   `scripts` (trusted markup the page itself built). Slots are filled in
//!   one pass over the template, so no inserted value is ever re-scanned.
//! - `static/app.css` is the one stylesheet. Page-specific rules go into the
//!   same file under a commented section. The shell links it as
//!   `/static/app.css?v=<content hash>`, so a stylesheet cached by a browser
//!   is never paired with newer markup after a deploy.
//! - [`assets::ASSETS`] is the table of everything served under `/static/`.
//!   A page that ships a script adds it there (bytes embedded at build
//!   time); no request ever reads the filesystem.
//! - No external request is made on first paint: no CDN, no web fonts
//!   (docs/DESIGN.md §6).
//! - Every page response goes through [`html_response`], which takes the
//!   Content-Security-Policy as a parameter. [`CSP_NO_SCRIPT`] is the default
//!   and denies scripts outright. A page that needs JavaScript (the desk,
//!   PR-11) defines its own policy constant next to the page: copy
//!   `CSP_NO_SCRIPT`, add `script-src 'self'` and `connect-src 'self'` only,
//!   serve the script as an asset file, and pass the constant to
//!   `html_response`. `unsafe-inline` and `unsafe-eval` are never an option;
//!   put no JavaScript in `scripts` that is not a `<script src>` to an asset.
//! - Dynamic values are escaped with [`escape::escape`] and placed only in
//!   element text, never in an attribute, URL, or script.

pub mod assets;
pub mod escape;
pub mod layout;
pub mod receipt_page;
pub mod spa;

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

/// Routes of the web pages. Merged into the application router.
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/receipt/{id}", get(receipt_page::receipt_page))
        .route("/static/{*path}", get(assets::static_asset))
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
