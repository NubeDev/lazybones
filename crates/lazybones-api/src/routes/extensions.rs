//! `/extensions` — the backend WASM extension surface (design §3.6) plus the
//! frontend asset proxy (§4.3).
//!
//! Installing an extension is the most privileged admin action on the REST
//! surface — it runs arbitrary sandboxed code and later grants it host
//! capabilities — so every mutation requires the loop-only
//! [`Capability::Extension`]. **Reads are open** (`GET /extensions`,
//! `GET /extensions/:id`, and the frontend proxy), like `/assets`/`/tasks`: the UI
//! lists installed extensions and the host imports each enabled remote's bundle
//! without a token.
//!
//! ## Where the bytes live (mirrors `/assets`)
//!
//! The `.wasm` component is a content-addressed [`BlobStore`](lazybones_store::BlobStore)
//! blob keyed by its sha256 under the `extensions` project; the store
//! [`Extension`] row is metadata only (design §3.5). The optional frontend remote
//! (its `remoteEntry.js` + chunks) lives under the `ext-frontend/<id>` project,
//! keyed by the in-bundle path, and is served back by [`frontend_asset`].
//!
//! ## Two indexes, one authority
//!
//! The durable store row is **authoritative**; the in-memory
//! [`Registry`](lazybones_ext::Registry) is the dispatch index keyed by exported
//! WIT interface. Every mutation writes the store first and then mirrors the
//! decision into the registry (best-effort — the registry is rebuilt from the
//! store on boot, so a post-restart miss is harmless).

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::http::header::CONTENT_TYPE;
use axum::response::{IntoResponse, Response};
use lazybones_auth::Capability;
use lazybones_ext::{
    Capability as ExtCapability, DiffStat, GateCheckHost, GateInput, GateOutcome, InstallRequest,
    Verdict, VerdictKind, dispatch::gate_verdict_fail_closed, validate_grant,
};
use lazybones_store::{Extension, ExtensionSource, StoreError, sha256_hex};
use serde::{Deserialize, Serialize};

use crate::error::{ApiError, ApiResult};
use crate::extract::Session;
use crate::state::AppState;

/// `?enabled=&frontend=` filter for the extension listing (design §4.3: the host
/// fetches `GET /extensions?frontend=1` on boot). Both are truthy-string flags.
#[derive(Debug, Default, Deserialize)]
pub struct ListQuery {
    /// Narrow to enabled extensions only when truthy.
    #[serde(default)]
    pub enabled: Option<String>,
    /// Narrow to extensions that ship a frontend remote when truthy.
    #[serde(default)]
    pub frontend: Option<String>,
}

/// `?id=` install override: the friendly id to register under. Omitted derives a
/// stable `ext-<sha16>` from the component bytes (so re-uploading identical bytes
/// collides rather than silently double-installing).
#[derive(Debug, Default, Deserialize)]
pub struct InstallQuery {
    /// The id to install under; defaults to `ext-<first-16-of-sha256>`.
    #[serde(default)]
    pub id: Option<String>,
}

/// `POST /extensions` JSON body (the URL-install path). The upload path sends the
/// raw `.wasm` bytes instead (distinguished by `Content-Type`).
#[derive(Debug, Deserialize)]
pub struct InstallUrlBody {
    /// `http(s)` URL the component is fetched from; retained as the source for
    /// provenance / re-fetch (design §3.5).
    pub url: String,
}

/// `POST /extensions/:id/grants` body: the capabilities an admin allows. Validated
/// `granted ⊆ requested` (and no deferred cap) before it is persisted.
#[derive(Debug, Deserialize)]
pub struct GrantsBody {
    /// The capability wire strings to grant (e.g. `["log", "store-read"]`).
    #[serde(default)]
    pub granted_caps: Vec<String>,
}

/// `POST /extensions/:id/invoke` body: a manual/test invocation of one named
/// export. v1 supports the `gate-check` export only.
#[derive(Debug, Deserialize)]
pub struct InvokeBody {
    /// The WIT export to invoke (must be one the extension declares; v1: `gate-check`).
    pub export: String,
    /// The input for that export. For `gate-check` this is a [`GateInputBody`].
    #[serde(default)]
    pub input: GateInputBody,
}

/// The `gate-check` input projected from JSON (the generated WIT record is not
/// serde-aware, so this is the wire shape mapped into [`GateInput`]).
#[derive(Debug, Default, Deserialize)]
pub struct GateInputBody {
    /// Stable id of the task being landed.
    #[serde(default)]
    pub task_id: String,
    /// One-line human summary of the change under evaluation.
    #[serde(default)]
    pub task_summary: String,
    /// Rolled-up diff statistics for the candidate worktree.
    #[serde(default)]
    pub diff: DiffStatBody,
}

/// The `diff-stat` sub-record of [`GateInputBody`].
#[derive(Debug, Default, Deserialize)]
pub struct DiffStatBody {
    /// Files touched by the change.
    #[serde(default)]
    pub files_changed: u32,
    /// Inserted lines across all files.
    #[serde(default)]
    pub insertions: u32,
    /// Deleted lines across all files.
    #[serde(default)]
    pub deletions: u32,
}

/// `POST /extensions/:id/invoke` response: the guest verdict (or the fail-closed
/// verdict a fault maps to) plus the measured cold-instantiation latency.
#[derive(Debug, Serialize)]
pub struct InvokeResponse {
    /// The export that was invoked.
    pub export: String,
    /// The verdict outcome (`pass` / `fail` / `skip`).
    pub verdict: VerdictView,
    /// Microseconds spent instantiating the component for this call; `None` if the
    /// invocation faulted before instantiation completed (design §3.4 flags cold
    /// instantiation as a measured P0 input).
    pub instantiation_micros: Option<u128>,
    /// Whether the verdict came from a host-boundary fault mapped fail-closed
    /// (rather than a clean guest return) — surfaced so a test-invoke shows the
    /// gate's actual landing behaviour.
    pub faulted: bool,
}

/// JSON projection of a gate-check [`Verdict`] (the generated record is not serde).
#[derive(Debug, Serialize)]
pub struct VerdictView {
    /// `pass` / `fail` / `skip`.
    pub kind: &'static str,
    /// Human-readable explanation surfaced to the operator.
    pub message: String,
}

/// Whether a truthy-string query flag (`1`, `true`, `yes`, `on`) is set.
fn truthy(flag: &Option<String>) -> bool {
    matches!(
        flag.as_deref().map(str::trim).map(str::to_ascii_lowercase).as_deref(),
        Some("1" | "true" | "yes" | "on")
    )
}

/// `GET /extensions` — list installed extensions (open read). `?enabled=1` narrows
/// to the active ones; `?frontend=1` to those shipping a UI remote (design §4.3).
pub async fn list_extensions(
    State(state): State<AppState>,
    Query(q): Query<ListQuery>,
) -> ApiResult<Json<Vec<Extension>>> {
    let mut list = state.store.list_extensions(truthy(&q.enabled)).await?;
    if truthy(&q.frontend) {
        list.retain(|e| e.frontend.is_some());
    }
    Ok(Json(list))
}

/// `GET /extensions/:id` — one extension's metadata (open read).
pub async fn get_extension(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<Extension>> {
    let ext = state
        .store
        .get_extension(&id)
        .await?
        .ok_or(StoreError::ExtensionNotFound(id))?;
    Ok(Json(ext))
}

/// `POST /extensions` — install an extension. Requires [`Capability::Extension`].
///
/// Two transports, distinguished by `Content-Type`:
/// - **upload**: the raw `.wasm` component as the request body (the default).
/// - **url**: a JSON body `{ "url": "https://…" }`; the daemon fetches the bytes
///   (shelling out to `curl`, like the rest of the codebase shells out to
///   `gh`/`git`).
///
/// The embedded manifest is parsed + validated by the registry (the source of
/// truth for identity/caps — §3.5); the extension installs **disabled with no
/// grants** (default-deny — §3.3), to be reviewed then enabled/granted separately.
pub async fn install_extension(
    State(state): State<AppState>,
    session: Session,
    Query(q): Query<InstallQuery>,
    headers: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<Extension>> {
    session.require(Capability::Extension, "extension", "extension")?;

    let is_json = headers
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|ct| ct.starts_with("application/json"));

    let (bytes, source) = if is_json {
        let req: InstallUrlBody = serde_json::from_slice(&body)
            .map_err(|e| ApiError::bad_request(format!("invalid install body: {e}")))?;
        let bytes = fetch_url(&req.url).await?;
        (bytes, ExtensionSource::Url(req.url))
    } else {
        (body.to_vec(), ExtensionSource::Upload)
    };

    if bytes.is_empty() {
        return Err(ApiError::bad_request("extension component must not be empty"));
    }

    let sha = sha256_hex(&bytes);
    let id = q.id.unwrap_or_else(|| format!("ext-{}", &sha[..sha.len().min(16)]));
    let created_at = state.store.now();

    // Validate + register into the dispatch index first (the registry owns manifest
    // parsing + the grant policy). Installs are default-deny: empty grants, disabled.
    let to_store = {
        let mut registry = state.extensions().write().expect("ext registry poisoned");
        let record = registry.install(InstallRequest {
            id: id.clone(),
            component: &bytes,
            granted_caps: Vec::new(),
            source,
            expected_sha256: None,
            enabled: false,
            created_at,
            record: None,
        })?;
        record.to_store_extension()
    };

    // Persist the metadata row, then the bytes. If the row write loses a race (the
    // id was taken between the registry insert and now), roll the registry back so
    // the two indexes never diverge.
    match state.store.create_extension(&to_store).await {
        Ok(ext) => {
            state
                .assets
                .put(&ext.wasm_sha256, Some("extensions"), &bytes)
                .await?;
            Ok(Json(ext))
        }
        Err(e) => {
            state.extensions().write().expect("ext registry poisoned").remove(&id);
            Err(e.into())
        }
    }
}

/// `DELETE /extensions/:id` — uninstall: drop the row, the bytes, and the dispatch
/// index entry. Requires [`Capability::Extension`]. Returns whether it existed.
pub async fn delete_extension(
    State(state): State<AppState>,
    session: Session,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    session.require(Capability::Extension, "extension", &id)?;
    if let Some(ext) = state.store.get_extension(&id).await? {
        let existed = state.store.delete_extension(&id).await?;
        // Best-effort byte cleanup (the id is not sha-derived in general, but the
        // wasm blob is keyed by its own content address and unshared here).
        let _ = state.assets.delete(&ext.wasm_sha256, Some("extensions")).await;
        state.extensions().write().expect("ext registry poisoned").remove(&id);
        Ok(Json(serde_json::json!({ "deleted": existed })))
    } else {
        Ok(Json(serde_json::json!({ "deleted": false })))
    }
}

/// `POST /extensions/:id/enable` — activate an extension (eligible for dispatch).
/// Requires [`Capability::Extension`].
pub async fn enable_extension(
    State(state): State<AppState>,
    session: Session,
    Path(id): Path<String>,
) -> ApiResult<Json<Extension>> {
    set_enabled(&state, &session, &id, true).await
}

/// `POST /extensions/:id/disable` — deactivate an extension (stays installed, not
/// dispatched). Requires [`Capability::Extension`].
pub async fn disable_extension(
    State(state): State<AppState>,
    session: Session,
    Path(id): Path<String>,
) -> ApiResult<Json<Extension>> {
    set_enabled(&state, &session, &id, false).await
}

/// Shared enable/disable: write the store (authoritative), then mirror into the
/// dispatch index.
async fn set_enabled(
    state: &AppState,
    session: &Session,
    id: &str,
    enabled: bool,
) -> ApiResult<Json<Extension>> {
    session.require(Capability::Extension, "extension", id)?;
    let ext = state.store.set_extension_enabled(id, enabled).await?;
    state
        .extensions()
        .write()
        .expect("ext registry poisoned")
        .set_enabled(id, enabled);
    Ok(Json(ext))
}

/// `POST /extensions/:id/grants` — set an extension's granted capabilities.
/// Requires [`Capability::Extension`]. Enforces `granted ⊆ requested` (and no
/// deferred cap) against the row's manifest-declared requests before persisting,
/// then mirrors the grant into the dispatch index (design §3.5).
pub async fn set_grants(
    State(state): State<AppState>,
    session: Session,
    Path(id): Path<String>,
    Json(body): Json<GrantsBody>,
) -> ApiResult<Json<Extension>> {
    session.require(Capability::Extension, "extension", &id)?;
    let ext = state
        .store
        .get_extension(&id)
        .await?
        .ok_or_else(|| StoreError::ExtensionNotFound(id.clone()))?;

    let requested = parse_caps(&ext.requested_caps)?;
    let granted = parse_caps(&body.granted_caps)?;
    validate_grant(&requested, &granted).map_err(|e| ApiError::Extension(e.to_string()))?;

    let updated = state
        .store
        .set_extension_grants(&id, body.granted_caps.clone())
        .await?;
    state
        .extensions()
        .write()
        .expect("ext registry poisoned")
        .set_grants(&id, granted);
    Ok(Json(updated))
}

/// `POST /extensions/:id/invoke` — manually/test-invoke a named export.
/// Requires [`Capability::Extension`]. v1 supports the `gate-check` export only;
/// the guest runs under the full fuel/epoch/memory/timeout regime and any
/// host-boundary fault is reported as the fail-closed verdict it would land as.
pub async fn invoke_extension(
    State(state): State<AppState>,
    session: Session,
    Path(id): Path<String>,
    Json(body): Json<InvokeBody>,
) -> ApiResult<Json<InvokeResponse>> {
    session.require(Capability::Extension, "extension", &id)?;

    let ext = state
        .store
        .get_extension(&id)
        .await?
        .ok_or_else(|| StoreError::ExtensionNotFound(id.clone()))?;

    if !ext.exports.contains(&body.export) {
        return Err(ApiError::bad_request(format!(
            "extension `{id}` does not export `{}`",
            body.export
        )));
    }
    if body.export != "gate-check" {
        return Err(ApiError::bad_request(format!(
            "export `{}` is not test-invokable in v1 (only `gate-check`)",
            body.export
        )));
    }

    let bytes = state
        .assets
        .get(&ext.wasm_sha256, Some("extensions"))
        .await?;

    let input = GateInput {
        task_id: body.input.task_id,
        task_summary: body.input.task_summary,
        diff: DiffStat {
            files_changed: body.input.diff.files_changed,
            insertions: body.input.diff.insertions,
            deletions: body.input.diff.deletions,
        },
    };

    // Compile + evaluate; fold a load fault into the same fail-closed path so the
    // response always shows what the gate would do.
    let engine = state.ext_engine().clone();
    let result = match GateCheckHost::from_bytes(engine, &bytes) {
        Ok(host) => host.evaluate(input).await,
        Err(fault) => Err(fault),
    };

    let (verdict, instantiation, faulted) = match result {
        Ok(GateOutcome {
            verdict,
            instantiation,
        }) => (verdict, Some(instantiation.as_micros()), false),
        Err(fault) => (gate_verdict_fail_closed(Err(fault)), None, true),
    };

    Ok(Json(InvokeResponse {
        export: body.export,
        verdict: verdict_view(&verdict),
        instantiation_micros: instantiation,
        faulted,
    }))
}

/// `GET /extensions/:id/frontend/*path` — serve a file from an enabled extension's
/// federated remote bundle (design §4.3). Open read: the Module Federation runtime
/// imports `remoteEntry.js` + chunks without a token. 404s for an unknown/disabled
/// extension, one with no frontend, or a path not present in its bundle.
pub async fn frontend_asset(
    State(state): State<AppState>,
    Path((id, path)): Path<(String, String)>,
) -> ApiResult<Response> {
    let ext = state
        .store
        .get_extension(&id)
        .await?
        .ok_or_else(|| StoreError::ExtensionNotFound(id.clone()))?;
    // Only enabled extensions with a UI half serve a bundle.
    if !ext.enabled || ext.frontend.is_none() {
        return Err(ApiError::NotFound);
    }
    // Reject path traversal / empty segments before they reach the blob key.
    if path
        .split('/')
        .any(|seg| seg.is_empty() || seg == "." || seg == "..")
    {
        return Err(ApiError::bad_request("invalid frontend asset path"));
    }

    let project = format!("ext-frontend/{id}");
    let bytes = state.assets.get(&path, Some(&project)).await?;
    Ok(([(CONTENT_TYPE, frontend_content_type(&path))], bytes).into_response())
}

/// Fetch component bytes from an `http(s)` URL by shelling out to `curl` (no new
/// HTTP-client dependency — consistent with the codebase's `gh`/`git` shell-outs).
async fn fetch_url(url: &str) -> ApiResult<Vec<u8>> {
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err(ApiError::bad_request("install url must be http(s)"));
    }
    let output = tokio::process::Command::new("curl")
        .args(["-fsSL", "--max-time", "60", url])
        .output()
        .await
        .map_err(|e| ApiError::Internal(format!("failed to spawn curl: {e}")))?;
    if !output.status.success() {
        return Err(ApiError::bad_request(format!(
            "failed to fetch extension from {url}: curl exited {}",
            output.status
        )));
    }
    Ok(output.stdout)
}

/// Parse capability wire strings into typed [`ExtCapability`]s, surfacing the first
/// unknown one as a `400`.
fn parse_caps(caps: &[String]) -> ApiResult<Vec<ExtCapability>> {
    caps.iter()
        .map(|c| ExtCapability::parse(c).map_err(|e| ApiError::Extension(e.to_string())))
        .collect()
}

/// Project a generated gate-check [`Verdict`] into its JSON view.
fn verdict_view(verdict: &Verdict) -> VerdictView {
    let kind = match verdict.kind {
        VerdictKind::Pass => "pass",
        VerdictKind::Fail => "fail",
        VerdictKind::Skip => "skip",
    };
    VerdictView {
        kind,
        message: verdict.message.clone(),
    }
}

/// Guess a `Content-Type` for a frontend bundle file from its extension. Defaults
/// to `application/octet-stream`. Notably maps `.js`/`.mjs` and `.wasm` correctly
/// so the Module Federation runtime and any wasm chunks load.
fn frontend_content_type(path: &str) -> &'static str {
    let ext = path.rsplit('.').next().unwrap_or("");
    match ext.to_ascii_lowercase().as_str() {
        "js" | "mjs" | "cjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" | "map" => "application/json; charset=utf-8",
        "html" | "htm" => "text/html; charset=utf-8",
        "wasm" => "application/wasm",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        _ => "application/octet-stream",
    }
}
