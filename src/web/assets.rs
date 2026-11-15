//! Static assets, served from memory out of a fixed table.
//!
//! Nothing here touches the filesystem at request time: the bytes are
//! embedded at build time and a request path is only ever compared against
//! the table's keys, so there is no path to traverse.

use std::sync::OnceLock;

use axum::extract::Path;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};

use crate::receipt::sha256_hex;

pub struct Asset {
    /// Path below `/static/`.
    pub path: &'static str,
    pub content_type: &'static str,
    pub bytes: &'static [u8],
}

/// Every asset the server can serve. A later page adds its script or
/// stylesheet here.
pub const ASSETS: &[Asset] = &[Asset {
    path: "app.css",
    content_type: "text/css; charset=utf-8",
    bytes: include_bytes!("../../static/app.css"),
}];

/// A short content hash of the stylesheet, computed once. The page links
/// `/static/app.css?v=<this>`, so a stylesheet cached for an hour is never
/// paired with newer markup after a deploy.
pub fn css_version() -> &'static str {
    static VERSION: OnceLock<String> = OnceLock::new();
    VERSION.get_or_init(|| {
        let css = ASSETS
            .iter()
            .find(|asset| asset.path == "app.css")
            .map(|asset| asset.bytes)
            .unwrap_or_default();
        sha256_hex(css)[..16].to_string()
    })
}

pub async fn static_asset(Path(path): Path<String>) -> Response {
    match ASSETS.iter().find(|asset| asset.path == path) {
        Some(asset) => (
            [
                (header::CONTENT_TYPE, asset.content_type),
                (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
                (header::CACHE_CONTROL, "public, max-age=3600"),
                (header::CONTENT_SECURITY_POLICY, "default-src 'none'"),
            ],
            asset.bytes,
        )
            .into_response(),
        None => (
            StatusCode::NOT_FOUND,
            [
                (header::CONTENT_TYPE, "text/plain; charset=utf-8"),
                (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
                (header::CACHE_CONTROL, "no-store"),
            ],
            "not found",
        )
            .into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_the_sha256_prefix_of_the_stylesheet() {
        let css = include_bytes!("../../static/app.css");
        assert_eq!(css_version(), &sha256_hex(css)[..16]);
    }

    #[test]
    fn asset_paths_are_flat_and_unique() {
        for (index, asset) in ASSETS.iter().enumerate() {
            assert!(!asset.path.contains('/') && !asset.path.contains(".."));
            assert!(
                ASSETS[index + 1..]
                    .iter()
                    .all(|other| other.path != asset.path)
            );
        }
    }
}
