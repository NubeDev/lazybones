//! Cross-origin policy for the REST surface.
//!
//! The dashboard UI runs on a *different origin* than the daemon — the browser
//! dev server is `http://localhost:51840` and the desktop shell serves from
//! `tauri://localhost`, while the API binds `127.0.0.1:46787`. Without CORS the
//! browser blocks every cross-origin response (curl is unaffected, which is why
//! the daemon looked healthy from the shell but the UI showed everything
//! "offline"). lazybones is a single-user local tool authenticated by a bearer
//! token (not cookies), so a permissive policy is appropriate and safe: allow
//! any origin, the methods the routes use, and the `authorization` header.

use axum::http::{HeaderName, Method};
use tower_http::cors::{Any, CorsLayer};

/// A permissive CORS layer for the local dashboard (any origin, bearer auth).
pub fn cors_layer() -> CorsLayer {
    CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            // `PATCH /tasks/:id` (edit a task, set worktree mode) is browser-only;
            // omitting it here made every cross-origin PATCH fail its preflight.
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([
            HeaderName::from_static("authorization"),
            HeaderName::from_static("content-type"),
        ])
}
