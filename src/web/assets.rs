//! Static assets, served from memory out of a fixed table.
//!
//! Nothing here touches the filesystem at request time: the bytes are
//! embedded at build time and a request path is only ever compared against
//! the table's keys, so there is no path to traverse.
//!
//! Two families live here. `app.css` styles the server-rendered pages. The
//! `spa.*` set is the compiled frontend (`frontend/dist/`, docs/DESIGN.md
//! §6) — rebuilt with `pnpm --dir frontend build` whenever the desk changes;
//! the binary embeds whatever is in `dist` at `cargo build` time. Fonts are
//! self-hosted woff2 (SIL OFL, see THIRD_PARTY_NOTICES.md).

use std::sync::OnceLock;

use axum::extract::Path;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};

use crate::receipt::sha256_hex;

pub struct Asset {
    /// Path below `/static/`. Flat, or under `fonts/`; never `..` (enforced
    /// by a test, because the router hands the raw path here).
    pub path: &'static str,
    pub content_type: &'static str,
    /// Per-asset policy: a stylesheet declares its own font fetches, a
    /// script fetches nothing. The default `default-src 'none'` stays the
    /// base of every entry.
    pub csp: &'static str,
    /// Stable names, so cache lifetime is the cache-busting story:
    /// shell-referenced bundles get a short window, fonts a long one.
    pub cache: &'static str,
    pub bytes: &'static [u8],
}

const NO_EXTERNAL_FETCHES: &str = "default-src 'none'";

/// Every asset the server can serve. A later page adds its script or
/// stylesheet here.
pub const ASSETS: &[Asset] = &[
    Asset {
        path: "app.css",
        content_type: "text/css; charset=utf-8",
        csp: NO_EXTERNAL_FETCHES,
        cache: "public, max-age=3600",
        bytes: include_bytes!("../../static/app.css"),
    },
    Asset {
        path: "receipt-verify.js",
        content_type: "text/javascript; charset=utf-8",
        // The script makes no request, so it asks for nothing but itself.
        csp: "default-src 'none'; script-src 'self'",
        cache: "public, max-age=3600",
        bytes: include_bytes!("../../static/receipt-verify.js"),
    },
    Asset {
        path: "spa.js",
        content_type: "text/javascript; charset=utf-8",
        csp: "default-src 'none'; script-src 'self'",
        cache: "public, max-age=3600",
        bytes: include_bytes!("../../frontend/dist/spa.js"),
    },
    Asset {
        path: "spa.css",
        content_type: "text/css; charset=utf-8",
        // The desk stylesheet declares the @font-face set, so it may fetch
        // fonts; nothing else.
        csp: "default-src 'none'; style-src 'self'; font-src 'self'",
        cache: "public, max-age=3600",
        bytes: include_bytes!("../../frontend/dist/spa.css"),
    },
    Asset {
        path: "fonts/ibm-plex-sans-latin-400-normal.woff2",
        content_type: "font/woff2",
        csp: NO_EXTERNAL_FETCHES,
        cache: "public, max-age=604800",
        bytes: include_bytes!("../../frontend/dist/fonts/ibm-plex-sans-latin-400-normal.woff2"),
    },
    Asset {
        path: "fonts/ibm-plex-sans-latin-500-normal.woff2",
        content_type: "font/woff2",
        csp: NO_EXTERNAL_FETCHES,
        cache: "public, max-age=604800",
        bytes: include_bytes!("../../frontend/dist/fonts/ibm-plex-sans-latin-500-normal.woff2"),
    },
    Asset {
        path: "fonts/ibm-plex-sans-latin-600-normal.woff2",
        content_type: "font/woff2",
        csp: NO_EXTERNAL_FETCHES,
        cache: "public, max-age=604800",
        bytes: include_bytes!("../../frontend/dist/fonts/ibm-plex-sans-latin-600-normal.woff2"),
    },
    Asset {
        path: "fonts/ibm-plex-sans-latin-700-normal.woff2",
        content_type: "font/woff2",
        csp: NO_EXTERNAL_FETCHES,
        cache: "public, max-age=604800",
        bytes: include_bytes!("../../frontend/dist/fonts/ibm-plex-sans-latin-700-normal.woff2"),
    },
    Asset {
        path: "fonts/ibm-plex-mono-latin-400-normal.woff2",
        content_type: "font/woff2",
        csp: NO_EXTERNAL_FETCHES,
        cache: "public, max-age=604800",
        bytes: include_bytes!("../../frontend/dist/fonts/ibm-plex-mono-latin-400-normal.woff2"),
    },
    Asset {
        path: "fonts/ibm-plex-mono-latin-500-normal.woff2",
        content_type: "font/woff2",
        csp: NO_EXTERNAL_FETCHES,
        cache: "public, max-age=604800",
        bytes: include_bytes!("../../frontend/dist/fonts/ibm-plex-mono-latin-500-normal.woff2"),
    },
    // The fonts ride the SIL OFL 1.1; the license text ships next to them.
    Asset {
        path: "fonts/OFL-1.1.txt",
        content_type: "text/plain; charset=utf-8",
        csp: NO_EXTERNAL_FETCHES,
        cache: "public, max-age=604800",
        bytes: include_bytes!("../../frontend/public/fonts/OFL-1.1.txt"),
    },
];

/// A short content hash of the server pages' stylesheet, computed once. The
/// shell links `/static/app.css?v=<this>`, so a stylesheet cached for an
/// hour is never paired with newer markup after a deploy.
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

/// A short content hash of the embedded asset `path`, for a `?v=` cache
/// buster. `None` for a path that is not in [`ASSETS`].
pub fn asset_version(path: &str) -> Option<String> {
    ASSETS
        .iter()
        .find(|asset| asset.path == path)
        .map(|asset| sha256_hex(asset.bytes)[..16].to_string())
}

pub async fn static_asset(Path(path): Path<String>) -> Response {
    match ASSETS.iter().find(|asset| asset.path == path) {
        Some(asset) => (
            [
                (header::CONTENT_TYPE, asset.content_type),
                (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
                (header::CACHE_CONTROL, asset.cache),
                (header::CONTENT_SECURITY_POLICY, asset.csp),
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
                (header::CONTENT_SECURITY_POLICY, NO_EXTERNAL_FETCHES),
            ],
            "not found",
        )
            .into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `fonts/<name>` is allowed (the woff2 set); anything else must be one
    /// flat segment. In all cases `..` and absolute paths are hostile and a
    /// collision would shadow one asset with another.
    #[test]
    fn asset_paths_are_flat_or_fonts_and_unique() {
        for (index, asset) in ASSETS.iter().enumerate() {
            assert!(
                !asset.path.contains(".."),
                "{} must not traverse",
                asset.path
            );
            assert!(
                !asset.path.starts_with('/'),
                "{} must be relative",
                asset.path
            );
            let legal = asset.path.split_once('/').is_none_or(|(dir, rest)| {
                dir == "fonts" && !rest.is_empty() && !rest.contains('/')
            });
            assert!(legal, "{} must be flat or fonts/<name>", asset.path);
            assert!(
                ASSETS[index + 1..]
                    .iter()
                    .all(|other| other.path != asset.path),
                "{} is listed twice",
                asset.path
            );
        }
    }

    #[test]
    fn every_asset_denies_external_fetches_by_default() {
        for asset in ASSETS {
            assert!(
                asset.csp.contains("default-src 'none'"),
                "{} must base its policy on default-src 'none'",
                asset.path
            );
            assert!(!asset.csp.contains("http"), "{}", asset.path);
        }
    }

    #[test]
    fn the_spa_bundle_and_stylesheet_exist_in_the_frontend_build() {
        for path in [
            "spa.js",
            "spa.css",
            "fonts/ibm-plex-sans-latin-400-normal.woff2",
        ] {
            let asset = ASSETS
                .iter()
                .find(|asset| asset.path == path)
                .unwrap_or_else(|| panic!("{path} missing from ASSETS"));
            assert!(
                !asset.bytes.is_empty(),
                "{path} embeds nothing — run `pnpm --dir frontend build` first"
            );
        }
    }

    #[test]
    fn version_is_the_sha256_prefix_of_the_stylesheet() {
        let css = include_bytes!("../../static/app.css");
        assert_eq!(css_version(), &sha256_hex(css)[..16]);
    }
}
