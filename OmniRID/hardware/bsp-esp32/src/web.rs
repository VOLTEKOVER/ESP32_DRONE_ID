//! ESP-IDF HTTP server: serves the embedded web UI, API endpoints, and OTA.
//!
//! Port of the HTTP routing from `web_config.c` and `rid_ota.c`.  The pure
//! logic (JSON rendering, OTA validation, log ring) lives in `rid_app`.

use alloc::string::String;
use alloc::vec::Vec;
use embedded_svc::io::Read;
use esp_idf_svc as _;
use esp_idf_svc::http::server::EspHttpServer;
use esp_idf_svc::http::Method;
use esp_idf_svc::sys::EspError;
use rid_app::web;
use rid_app::webui;

use crate::SharedState;

/// Start the HTTP server with all API routes.
///
/// `EspHttpServer::new` starts the server immediately (there is no separate
/// `start()` step).  Each `fn_handler` closure returns `Result<(), EspIOError>`
/// and pushes the response body through `into_response`'s `message` argument.
pub fn start(state: &SharedState) -> Result<EspHttpServer<'static>, EspError> {
    let mut server = EspHttpServer::new(&Default::default()).expect("httpd start");

    // Serve embedded web UI assets.
    for asset in webui::ASSETS {
        let path = asset.path;
        server
            .fn_handler(path, Method::Get, move |req| {
                let content_type = asset.content_type;
                req.into_response(200, Some(asset.data), &[("Content-Type", content_type)])?;
                Ok(())
            })
            .expect("register handler");
    }

    // GET /api/config
    {
        let state_ptr = state as *const SharedState;
        server
            .fn_handler("/api/config", Method::Get, move |req| {
                let state = unsafe { &*state_ptr };
                let lock = state.ctl.lock();
                let json = lock.config_json();
                req.into_response(
                    200,
                    Some(json.as_str()),
                    &[("Content-Type", "application/json")],
                )?;
                Ok(())
            })
            .expect("register handler");
    }

    // POST /api/config
    {
        let state_ptr = state as *const SharedState;
        server
            .fn_handler("/api/config", Method::Post, move |mut req| {
                let state = unsafe { &*state_ptr };
                let mut body = Vec::new();
                let mut buf = [0u8; 1024];
                loop {
                    let n = req.read(&mut buf)?;
                    if n == 0 {
                        break;
                    }
                    body.extend_from_slice(&buf[..n]);
                }
                let json_str = alloc::str::from_utf8(&body).unwrap_or("{}");
                let result = {
                    let mut lock = state.ctl.lock();
                    if rid_app::json::apply_json(&mut lock.bsp_config, json_str) {
                        let outcome = lock.set_config(&lock.bsp_config.clone());
                        super::nvs_save(&lock.bsp_config);
                        if outcome.protocol_reinit_required {
                            // TODO: re-init UART
                        }
                        true
                    } else {
                        false
                    }
                };
                let resp = if result { r#"{"ok":true}"# } else { r#"{"error":"invalid config"}"# };
                req.into_response(
                    200,
                    Some(resp),
                    &[("Content-Type", "application/json")],
                )?;
                Ok(())
            })
            .expect("register handler");
    }

    // GET /api/status
    {
        let state_ptr = state as *const SharedState;
        server
            .fn_handler("/api/status", Method::Get, move |req| {
                let state = unsafe { &*state_ptr };
                let lock = state.ctl.lock();
                let json = lock.status_json();
                req.into_response(
                    200,
                    Some(json.as_str()),
                    &[("Content-Type", "application/json")],
                )?;
                Ok(())
            })
            .expect("register handler");
    }

    // GET /api/capabilities
    {
        server
            .fn_handler("/api/capabilities", Method::Get, move |req| {
                let json = crate::capabilities::capabilities_json();
                req.into_response(
                    200,
                    Some(json.as_str()),
                    &[("Content-Type", "application/json")],
                )?;
                Ok(())
            })
            .expect("register handler");
    }

    // POST /api/reset
    {
        let state_ptr = state as *const SharedState;
        server
            .fn_handler("/api/reset", Method::Post, move |req| {
                let state = unsafe { &*state_ptr };
                {
                    let mut lock = state.ctl.lock();
                    lock.factory_reset();
                    super::nvs_erase();
                }
                req.into_response(
                    200,
                    Some(r#"{"ok":true}"#),
                    &[("Content-Type", "application/json")],
                )?;
                Ok(())
            })
            .expect("register handler");
    }

    // GET /api/logs
    {
        let state_ptr = state as *const SharedState;
        server
            .fn_handler("/api/logs", Method::Get, move |req| {
                let state = unsafe { &*state_ptr };
                let lock = state.log_ring.lock();
                let mut buf = [0u8; web::LOG_BUF_SIZE];
                let n = lock.render_log_json(&mut buf);
                let json = String::from_utf8_lossy(&buf[..n]);
                req.into_response(
                    200,
                    Some(json.as_ref()),
                    &[("Content-Type", "application/json")],
                )?;
                Ok(())
            })
            .expect("register handler");
    }

    // POST /ota
    {
        server
            .fn_handler("/ota", Method::Post, move |mut req| {
                // Read body.
                let mut body = Vec::new();
                let mut buf = [0u8; 4096];
                loop {
                    let n = req.read(&mut buf)?;
                    if n == 0 {
                        break;
                    }
                    body.extend_from_slice(&buf[..n]);
                }
                // TODO: extract X-Expected-SHA256 and X-Signature headers from request
                let resp = r#"{"error":"OTA via API not yet wired - use /update"}"#;
                req.into_response(
                    200,
                    Some(resp),
                    &[("Content-Type", "application/json")],
                )?;
                Ok(())
            })
            .expect("register handler");
    }

    Ok(server)
}
