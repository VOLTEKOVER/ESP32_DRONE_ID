//! Embedded web UI assets (config.html + style.css + app.js), port of
//! `webui/` from the C firmware.  The files are included at compile time via
//! `include_str!`/`include_bytes!` so the BSP can serve them from flash
//! without needing a filesystem.  The JavaScript already speaks to the three
//! JSON endpoints (`/api/config`, `/api/status`, `/api/capabilities`) whose
//! payloads are produced by `app::controller`.
//!
//! Pure and host-testable: the module only provides static data and a
//! content-type registry; no HTTP server code lives here.

/// Content-type for the single-page config app.
pub const HTML_CONTENT_TYPE: &str = "text/html; charset=utf-8";

/// Content-type for the stylesheet.
pub const CSS_CONTENT_TYPE: &str = "text/css; charset=utf-8";

/// Content-type for the JavaScript.
pub const JS_CONTENT_TYPE: &str = "application/javascript; charset=utf-8";

/// The full config.html page (port of `webui/config.html`).
pub const CONFIG_HTML: &str = include_str!("../webui/config.html");

/// The full stylesheet (port of `webui/style.css`).
pub const STYLE_CSS: &str = include_str!("../webui/style.css");

/// The full application JavaScript (port of `webui/app.js`).
pub const APP_JS: &str = include_str!("../webui/app.js");

/// A single static asset that the web server can serve.
pub struct Asset {
    pub path: &'static str,
    pub content_type: &'static str,
    pub data: &'static str,
}

/// All built-in assets in the order the web server should register them.
pub const ASSETS: &[Asset] = &[
    Asset {
        path: "/",
        content_type: HTML_CONTENT_TYPE,
        data: CONFIG_HTML,
    },
    Asset {
        path: "/config.html",
        content_type: HTML_CONTENT_TYPE,
        data: CONFIG_HTML,
    },
    Asset {
        path: "/style.css",
        content_type: CSS_CONTENT_TYPE,
        data: STYLE_CSS,
    },
    Asset {
        path: "/app.js",
        content_type: JS_CONTENT_TYPE,
        data: APP_JS,
    },
];

/// Look up a static asset by its HTTP path.
/// Returns `None` for paths that are not a static web UI file (API endpoints
/// and OTA uploads are handled separately).
pub fn lookup(path: &str) -> Option<&'static Asset> {
    ASSETS.iter().find(|a| a.path == path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_assets_are_non_empty() {
        for asset in ASSETS {
            assert!(
                !asset.data.is_empty(),
                "asset {} is empty",
                asset.path,
            );
            assert!(
                !asset.content_type.is_empty(),
                "content-type for {} is empty",
                asset.path,
            );
        }
    }

    #[test]
    fn lookup_finds_all_registered_paths() {
        assert!(lookup("/").is_some());
        assert!(lookup("/config.html").is_some());
        assert!(lookup("/style.css").is_some());
        assert!(lookup("/app.js").is_some());
    }

    #[test]
    fn lookup_rejects_unknown_paths() {
        assert!(lookup("/api/config").is_none());
        assert!(lookup("/api/status").is_none());
        assert!(lookup("/api/capabilities").is_none());
        assert!(lookup("/ota").is_none());
        assert!(lookup("/unknown").is_none());
    }

    #[test]
    fn content_types_are_correct() {
        assert_eq!(lookup("/").unwrap().content_type, HTML_CONTENT_TYPE);
        assert_eq!(lookup("/style.css").unwrap().content_type, CSS_CONTENT_TYPE);
        assert!(lookup("/app.js").unwrap().content_type.contains("javascript"));
    }

    #[test]
    fn html_contains_expected_structure() {
        assert!(CONFIG_HTML.contains("<!DOCTYPE html>"));
        assert!(CONFIG_HTML.contains("tab-dashboard"));
        assert!(CONFIG_HTML.contains("tab-identity"));
        assert!(CONFIG_HTML.contains("tab-transmission"));
        assert!(CONFIG_HTML.contains("tab-hardware"));
        assert!(CONFIG_HTML.contains("tab-system"));
        assert!(CONFIG_HTML.contains("tab-firmware"));
    }

    #[test]
    fn js_references_the_three_api_endpoints() {
        assert!(APP_JS.contains("/api/config"));
        assert!(APP_JS.contains("/api/status"));
        assert!(APP_JS.contains("/api/reset"));
    }

    #[test]
    fn assets_count_matches_c_webui() {
        assert_eq!(ASSETS.len(), 4);
    }
}
