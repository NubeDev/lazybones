//! Extension tools — author vs. install, the sharp §6.3 split.
//!
//! `extension.scaffold` generates a `cargo-component` guest skeleton + a
//! `lazybones.ext.toml` manifest (+ an optional federated-remote skeleton) into a
//! repo/worktree — a *file-writing* authoring act, gated by `Author`/`Document`.
//! Reads (`extension.list`/`get`) need no capability. `extension.install`/
//! `set_grants`/`enable`/`disable`/`invoke` require the **loop-only**
//! [`Capability::Extension`]: installing sandboxed code and granting it host
//! capabilities is the single most privileged act on the surface, so an MCP agent
//! can *author* an extension's source but never self-install + self-grant it
//! (extension-system §3.3 trust boundary). The gated tools are real twins of the
//! `/extensions` routes — they call the same store verbs and mirror into the same
//! in-memory [`Registry`](lazybones_ext::Registry) — but the capability gate refuses
//! every management token before any of that runs.

use std::path::{Path as FsPath, PathBuf};

use rmcp::handler::server::tool::Extension;
use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::{tool, tool_router};
use serde_json::{Value, json};

use lazybones_auth::Capability;
use lazybones_ext::{
    Capability as ExtCapability, DiffStat, GateCheckHost, GateInput, GateOutcome, HostAllowlist,
    InstallRequest, Verdict, VerdictKind, WeatherHost, WeatherOutcome, WeatherQuery, WeatherResult,
    dispatch::gate_verdict_fail_closed, validate_grant,
};
use lazybones_store::{Extension as StoreExtension, ExtensionSource, StoreError, sha256_hex};

use crate::args::{
    ExtensionGrantsArgs, ExtensionInstallArgs, ExtensionInvokeArgs, ExtensionListArgs,
    ExtensionScaffoldArgs, IdArgs,
};
use crate::auth::authorization_header;
use crate::error::{McpError, McpResult};
use crate::server::McpServer;
use crate::tools::json;

/// The default extension point the scaffold's `run` body implements when the caller
/// names none — the v1 example export (`gate-check`), so a freshly scaffolded guest
/// compiles against the checked-in example shape.
const DEFAULT_EXTENSION_POINT: &str = "gate-check";

/// The hosts the `weather` guest's outbound `http-fetch` is bounded to — the twin of
/// the route's `WEATHER_ALLOWLIST` (design §3.3: `http-fetch` is allowlist-only).
const WEATHER_ALLOWLIST: [&str; 2] = ["geocoding-api.open-meteo.com", "api.open-meteo.com"];

#[tool_router(router = extensions_router, vis = "pub(crate)")]
impl McpServer {
    /// `extension.scaffold` — author a guest skeleton + manifest (+ optional frontend
    /// remote) into `<dir>/<id>/`. A *file-writing* authoring act, allowed for the
    /// `Author` **or** `Document` profile (§6.3): it only writes source — installing
    /// it (loop-only) is a separate, gated step. Returns the files written.
    #[tool(
        name = "extension.scaffold",
        description = "Author a cargo-component WASM guest skeleton + lazybones.ext.toml manifest (and an optional federated frontend-remote skeleton) into <dir>/<id>/. A file-writing authoring act (Author or Document); installing the built component stays loop-only."
    )]
    pub async fn extension_scaffold(
        &self,
        Extension(parts): Extension<http::request::Parts>,
        Parameters(args): Parameters<ExtensionScaffoldArgs>,
    ) -> McpResult<Json<Value>> {
        self.authorize_any(
            authorization_header(&parts),
            &[Capability::Author, Capability::Document],
        )?;
        let files = scaffold_extension(&args)?;
        json(files)
    }

    /// `extension.list` — list installed extensions (open read), twin of
    /// `GET /extensions`. `enabled` narrows to active ones; `frontend` to those
    /// shipping a UI remote (design §4.3).
    #[tool(
        name = "extension.list",
        description = "List installed extensions; optionally narrow to enabled-only and/or those shipping a frontend remote. No capability required (twin of GET /extensions)."
    )]
    pub async fn extension_list(
        &self,
        Parameters(args): Parameters<ExtensionListArgs>,
    ) -> McpResult<Json<Value>> {
        let mut list = self
            .store()
            .list_extensions(args.enabled)
            .await
            .map_err(McpError::from)?;
        if args.frontend {
            list.retain(|e| e.frontend.is_some());
        }
        json(list)
    }

    /// `extension.get` — one extension's metadata (open read), twin of
    /// `GET /extensions/:id`. `404` if unknown.
    #[tool(
        name = "extension.get",
        description = "Fetch one installed extension's metadata (identity, exports, requested/granted caps, enabled, frontend). No capability required (twin of GET /extensions/:id)."
    )]
    pub async fn extension_get(
        &self,
        Parameters(args): Parameters<IdArgs>,
    ) -> McpResult<Json<Value>> {
        json(self.require_extension(&args.id).await?)
    }

    /// `extension.install` — install an extension from a URL. The twin of
    /// `POST /extensions` (the URL path only — MCP carries JSON, never raw `.wasm`
    /// bytes, design §8). Requires the **loop-only** [`Capability::Extension`]: no
    /// management profile may install. Installs **disabled with no grants**
    /// (default-deny, §3.3).
    #[tool(
        name = "extension.install",
        description = "Install a WASM extension from an http(s) URL (default-deny: disabled, no grants). Requires the loop-only Extension capability — no management profile may install (twin of POST /extensions, url path)."
    )]
    pub async fn extension_install(
        &self,
        Extension(parts): Extension<http::request::Parts>,
        Parameters(args): Parameters<ExtensionInstallArgs>,
    ) -> McpResult<Json<Value>> {
        self.authorize(authorization_header(&parts), Capability::Extension)?;

        let bytes = fetch_url(&args.url).await?;
        if bytes.is_empty() {
            return Err(McpError::bad_request("extension component must not be empty").into());
        }
        let sha = sha256_hex(&bytes);
        let id = args
            .id
            .unwrap_or_else(|| format!("ext-{}", &sha[..sha.len().min(16)]));

        // Validate + register into the dispatch index first (the registry owns
        // manifest parsing + grant policy). Installs are default-deny: empty grants,
        // disabled — exactly the route's contract.
        let to_store = {
            let mut registry = self.ext_registry()?.write().expect("ext registry poisoned");
            let record = registry
                .install(InstallRequest {
                    id: id.clone(),
                    component: &bytes,
                    granted_caps: Vec::new(),
                    source: ExtensionSource::Url(args.url.clone()),
                    expected_sha256: None,
                    enabled: false,
                    created_at: self.store().now(),
                    record: None,
                })
                .map_err(McpError::from)?;
            record.to_store_extension()
        };

        // Persist the metadata row, then the bytes. If the row write loses a race,
        // roll the registry back so the two indexes never diverge (route parity).
        match self.store().create_extension(&to_store).await {
            Ok(ext) => {
                self.put_extension_bytes(&ext.wasm_sha256, &bytes).await?;
                json(ext)
            }
            Err(e) => {
                self.ext_registry()?
                    .write()
                    .expect("ext registry poisoned")
                    .remove(&id);
                Err(McpError::from(e).into())
            }
        }
    }

    /// `extension.set_grants` — set an extension's granted capabilities. The twin of
    /// `POST /extensions/:id/grants`: requires the **loop-only**
    /// [`Capability::Extension`]. Enforces `granted ⊆ requested` (and no deferred
    /// cap) before persisting, then mirrors into the dispatch index (§3.5).
    #[tool(
        name = "extension.set_grants",
        description = "Set an installed extension's granted host capabilities (granted ⊆ requested). Requires the loop-only Extension capability — no management profile may grant (twin of POST /extensions/:id/grants)."
    )]
    pub async fn extension_set_grants(
        &self,
        Extension(parts): Extension<http::request::Parts>,
        Parameters(args): Parameters<ExtensionGrantsArgs>,
    ) -> McpResult<Json<Value>> {
        self.authorize(authorization_header(&parts), Capability::Extension)?;
        let ext = self.require_extension(&args.id).await?;

        let requested = parse_caps(&ext.requested_caps)?;
        let granted = parse_caps(&args.granted_caps)?;
        validate_grant(&requested, &granted)
            .map_err(|e| McpError::bad_request(e.to_string()))?;

        let updated = self
            .store()
            .set_extension_grants(&args.id, args.granted_caps.clone())
            .await
            .map_err(McpError::from)?;
        self.ext_registry()?
            .write()
            .expect("ext registry poisoned")
            .set_grants(&args.id, granted);
        json(updated)
    }

    /// `extension.enable` — activate an extension (eligible for dispatch). The twin of
    /// `POST /extensions/:id/enable`: requires the **loop-only**
    /// [`Capability::Extension`].
    #[tool(
        name = "extension.enable",
        description = "Activate an installed extension (eligible for gate-check dispatch). Requires the loop-only Extension capability (twin of POST /extensions/:id/enable)."
    )]
    pub async fn extension_enable(
        &self,
        Extension(parts): Extension<http::request::Parts>,
        Parameters(args): Parameters<IdArgs>,
    ) -> McpResult<Json<Value>> {
        self.set_extension_enabled(authorization_header(&parts), &args.id, true)
            .await
    }

    /// `extension.disable` — deactivate an extension (stays installed, not
    /// dispatched). The twin of `POST /extensions/:id/disable`: requires the
    /// **loop-only** [`Capability::Extension`].
    #[tool(
        name = "extension.disable",
        description = "Deactivate an installed extension (stays installed, not dispatched). Requires the loop-only Extension capability (twin of POST /extensions/:id/disable)."
    )]
    pub async fn extension_disable(
        &self,
        Extension(parts): Extension<http::request::Parts>,
        Parameters(args): Parameters<IdArgs>,
    ) -> McpResult<Json<Value>> {
        self.set_extension_enabled(authorization_header(&parts), &args.id, false)
            .await
    }

    /// `extension.invoke` — manually/test-invoke a named export. The twin of
    /// `POST /extensions/:id/invoke`: requires the **loop-only**
    /// [`Capability::Extension`]. v1 supports `gate-check` and `weather`; the guest
    /// runs under the full fuel/epoch/memory/timeout regime and any host-boundary
    /// fault is caught (fail-closed verdict for `gate-check`, surfaced as `error` for
    /// `weather`).
    #[tool(
        name = "extension.invoke",
        description = "Test-invoke a named export of an installed extension (v1: gate-check, weather) under the full sandbox limits. Requires the loop-only Extension capability (twin of POST /extensions/:id/invoke)."
    )]
    pub async fn extension_invoke(
        &self,
        Extension(parts): Extension<http::request::Parts>,
        Parameters(args): Parameters<ExtensionInvokeArgs>,
    ) -> McpResult<Json<Value>> {
        self.authorize(authorization_header(&parts), Capability::Extension)?;
        let ext = self.require_extension(&args.id).await?;

        if !ext.exports.contains(&args.export) {
            return Err(McpError::bad_request(format!(
                "extension `{}` does not export `{}`",
                args.id, args.export
            ))
            .into());
        }

        let bytes = self.get_extension_bytes(&ext.wasm_sha256).await?;
        let engine = self.ext_engine()?;

        let response = match args.export.as_str() {
            "gate-check" => invoke_gate_check(engine, &bytes, args.input).await,
            "weather" => invoke_weather(engine, &bytes, &ext, args.input).await?,
            other => {
                return Err(McpError::bad_request(format!(
                    "export `{other}` is not test-invokable in v1 (only `gate-check`, `weather`)"
                ))
                .into());
            }
        };
        Ok(Json(response))
    }
}

// ---- shared helpers (the in-crate twin of the route module's privates) --------

impl McpServer {
    /// 404 unless the extension exists — the twin of the routes' explicit
    /// `ExtensionNotFound` mapping.
    async fn require_extension(&self, id: &str) -> Result<StoreExtension, McpError> {
        self.store()
            .get_extension(id)
            .await
            .map_err(McpError::from)?
            .ok_or_else(|| McpError::from(StoreError::ExtensionNotFound(id.to_owned())))
    }

    /// Shared enable/disable: gate on the loop-only capability, write the store
    /// (authoritative), then mirror the decision into the dispatch index — the twin
    /// of the route's `set_enabled`.
    async fn set_extension_enabled(
        &self,
        authorization: Option<&str>,
        id: &str,
        enabled: bool,
    ) -> McpResult<Json<Value>> {
        self.authorize(authorization, Capability::Extension)?;
        let ext = self
            .store()
            .set_extension_enabled(id, enabled)
            .await
            .map_err(McpError::from)?;
        self.ext_registry()?
            .write()
            .expect("ext registry poisoned")
            .set_enabled(id, enabled);
        json(ext)
    }

    /// Store an installed component's bytes under the content-addressed `extensions`
    /// project — the twin of the install route's `assets.put`. The asset blob store
    /// is always wired at mount; its absence (the unit-test path, unreachable past
    /// the capability gate) is an internal error.
    async fn put_extension_bytes(&self, sha: &str, bytes: &[u8]) -> Result<(), McpError> {
        self.assets()
            .ok_or_else(|| McpError::Internal("asset blob store not wired".to_owned()))?
            .put(sha, Some("extensions"), bytes)
            .await
            .map_err(McpError::from)
    }

    /// Fetch an installed component's bytes from the `extensions` project — the twin
    /// of the invoke route's `assets.get`.
    async fn get_extension_bytes(&self, sha: &str) -> Result<Vec<u8>, McpError> {
        self.assets()
            .ok_or_else(|| McpError::Internal("asset blob store not wired".to_owned()))?
            .get(sha, Some("extensions"))
            .await
            .map_err(McpError::from)
    }
}

/// Run the `gate-check` export, folding a load/host fault into the fail-closed
/// verdict so the response always shows what the gate would do — the twin of the
/// route's `invoke_gate_check`, projected to JSON.
async fn invoke_gate_check(engine: lazybones_ext::ExtEngine, bytes: &[u8], input: Value) -> Value {
    let gate: GateInputJson = serde_json::from_value(input).unwrap_or_default();
    let gate_input = GateInput {
        task_id: gate.task_id,
        task_summary: gate.task_summary,
        diff: DiffStat {
            files_changed: gate.diff.files_changed,
            insertions: gate.diff.insertions,
            deletions: gate.diff.deletions,
        },
    };

    let result = match GateCheckHost::from_bytes(engine, bytes) {
        Ok(host) => host.evaluate(gate_input).await,
        Err(fault) => Err(fault),
    };

    let (verdict, instantiation, faulted) = match result {
        Ok(GateOutcome {
            verdict,
            instantiation,
        }) => (verdict, Some(instantiation.as_micros()), false),
        Err(fault) => (gate_verdict_fail_closed(Err(fault)), None, true),
    };

    json!({
        "export": "gate-check",
        "verdict": verdict_view(&verdict),
        "instantiation_micros": instantiation,
        "faulted": faulted,
    })
}

/// Run the `weather` export. The guest does the outbound fetch itself, so this
/// **requires the `http-fetch` grant** (default-deny, §3.3) and bounds the guest to
/// [`WEATHER_ALLOWLIST`] — the twin of the route's `invoke_weather`, projected to
/// JSON.
async fn invoke_weather(
    engine: lazybones_ext::ExtEngine,
    bytes: &[u8],
    ext: &StoreExtension,
    input: Value,
) -> Result<Value, McpError> {
    if !ext.granted_caps.iter().any(|c| c == "http-fetch") {
        return Err(McpError::bad_request(
            "the `weather` export requires the `http-fetch` capability to be granted",
        ));
    }

    let parsed: WeatherInputJson = serde_json::from_value(input).unwrap_or_default();
    let query = WeatherQuery {
        location: parsed.location,
    };

    let allowlist = HostAllowlist::from_hosts(WEATHER_ALLOWLIST);
    let result = match WeatherHost::from_bytes(engine, bytes, allowlist) {
        Ok(host) => host.fetch(query).await,
        Err(fault) => Err(fault),
    };

    Ok(match result {
        Ok(WeatherOutcome {
            result,
            instantiation,
        }) => json!({
            "export": "weather",
            "weather": weather_view(&result),
            "instantiation_micros": instantiation.as_micros(),
            "faulted": false,
        }),
        Err(fault) => json!({
            "export": "weather",
            "weather": Value::Null,
            "instantiation_micros": Value::Null,
            "faulted": true,
            "error": fault.to_string(),
        }),
    })
}

/// Fetch component bytes from an `http(s)` URL by shelling out to `curl` (no new
/// HTTP-client dependency) — the twin of the install route's `fetch_url`.
async fn fetch_url(url: &str) -> Result<Vec<u8>, McpError> {
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err(McpError::bad_request("install url must be http(s)"));
    }
    let output = tokio::process::Command::new("curl")
        .args(["-fsSL", "--max-time", "60", url])
        .output()
        .await
        .map_err(|e| McpError::Internal(format!("failed to spawn curl: {e}")))?;
    if !output.status.success() {
        return Err(McpError::bad_request(format!(
            "failed to fetch extension from {url}: curl exited {}",
            output.status
        )));
    }
    Ok(output.stdout)
}

/// Parse capability wire strings into typed [`ExtCapability`]s, surfacing the first
/// unknown one as a bad request — the twin of the route's `parse_caps`.
fn parse_caps(caps: &[String]) -> Result<Vec<ExtCapability>, McpError> {
    caps.iter()
        .map(|c| ExtCapability::parse(c).map_err(|e| McpError::bad_request(e.to_string())))
        .collect()
}

/// JSON projection of a gate-check [`Verdict`] (the generated record is not serde) —
/// the twin of the route's `VerdictView`.
fn verdict_view(verdict: &Verdict) -> Value {
    let kind = match verdict.kind {
        VerdictKind::Pass => "pass",
        VerdictKind::Fail => "fail",
        VerdictKind::Skip => "skip",
    };
    json!({ "kind": kind, "message": verdict.message })
}

/// JSON projection of a [`WeatherResult`] (the generated record is not serde) — the
/// twin of the route's `WeatherView`.
fn weather_view(r: &WeatherResult) -> Value {
    json!({
        "location": r.location,
        "latitude": r.latitude,
        "longitude": r.longitude,
        "temperature_c": r.temperature_c,
        "wind_kph": r.wind_kph,
        "weather_code": r.weather_code,
        "description": r.description,
        "observed_at": r.observed_at,
        "error": r.error,
    })
}

/// The `gate-check` invoke input projected from JSON (the generated WIT record is
/// not serde-aware) — the twin of the route's `GateInputBody`.
#[derive(Debug, Default, serde::Deserialize)]
struct GateInputJson {
    #[serde(default)]
    task_id: String,
    #[serde(default)]
    task_summary: String,
    #[serde(default)]
    diff: DiffStatJson,
}

/// The `diff-stat` sub-record of [`GateInputJson`].
#[derive(Debug, Default, serde::Deserialize)]
struct DiffStatJson {
    #[serde(default)]
    files_changed: u32,
    #[serde(default)]
    insertions: u32,
    #[serde(default)]
    deletions: u32,
}

/// The `weather` invoke input projected from JSON — the twin of the route's
/// `WeatherInputBody`.
#[derive(Debug, Default, serde::Deserialize)]
struct WeatherInputJson {
    #[serde(default)]
    location: String,
}

// ---- scaffold (the file-writing authoring act, §6.3) --------------------------

/// Write a `cargo-component` guest skeleton + `lazybones.ext.toml` manifest (and an
/// optional federated frontend-remote skeleton) into `<dir>/<id>/`, returning the
/// relative paths written. Purely a file-writing act: it produces *source* the
/// operator later builds + installs (loop-only). The manifest it emits parses +
/// validates through [`lazybones_ext::Manifest`] so a scaffolded extension is
/// install-ready once compiled.
fn scaffold_extension(args: &ExtensionScaffoldArgs) -> Result<Value, McpError> {
    let id = args.id.trim();
    if id.is_empty() {
        return Err(McpError::bad_request("extension id must not be empty"));
    }
    // The id names a single directory segment + the cargo package; reject anything
    // that would escape `<dir>/<id>/` or break the package name.
    if id.contains('/') || id.contains('\\') || id.contains("..") {
        return Err(McpError::bad_request(
            "extension id must be a single path segment (no `/`, `\\`, or `..`)",
        ));
    }

    let name = args
        .name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(id);
    let description = args
        .description
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("A lazybones WASM extension.");
    let points: Vec<String> = if args.extension_points.is_empty() {
        vec![DEFAULT_EXTENSION_POINT.to_owned()]
    } else {
        args.extension_points.clone()
    };

    let root = FsPath::new(&args.dir).join(id);
    let mut written: Vec<String> = Vec::new();

    write_file(
        &root,
        "Cargo.toml",
        &guest_cargo_toml(id),
        &mut written,
    )?;
    write_file(
        &root,
        "src/lib.rs",
        &guest_lib_rs(name, description, &points),
        &mut written,
    )?;
    write_file(
        &root,
        "lazybones.ext.toml",
        &manifest_toml(name, &points, &args.requested_caps, args.frontend),
        &mut written,
    )?;
    write_file(&root, ".gitignore", "/target\nCargo.lock\n", &mut written)?;
    write_file(
        &root,
        "README.md",
        &readme_md(id, name, &points, args.frontend),
        &mut written,
    )?;

    if args.frontend {
        write_file(
            &root,
            "frontend/package.json",
            &frontend_package_json(id),
            &mut written,
        )?;
        write_file(
            &root,
            "frontend/vite.config.ts",
            &frontend_vite_config(id),
            &mut written,
        )?;
        write_file(
            &root,
            "frontend/src/Extension.tsx",
            FRONTEND_EXTENSION_TSX,
            &mut written,
        )?;
    }

    // Sanity-check the manifest we just emitted parses + validates, so a scaffolded
    // extension is never shipped with a manifest the installer would reject.
    let manifest_src = manifest_toml(name, &points, &args.requested_caps, args.frontend);
    lazybones_ext::Manifest::parse(&manifest_src)
        .map_err(|e| McpError::Internal(format!("scaffolded manifest failed to validate: {e}")))?;

    Ok(json!({
        "dir": root.display().to_string(),
        "id": id,
        "files": written,
    }))
}

/// Write one file under `root/rel`, creating parent dirs, and record `rel` in
/// `written`. A filesystem failure is an internal error (the act is authoring source
/// into a repo the agent was handed).
fn write_file(
    root: &FsPath,
    rel: &str,
    contents: &str,
    written: &mut Vec<String>,
) -> Result<(), McpError> {
    let path: PathBuf = root.join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| McpError::Internal(format!("create dir {}: {e}", parent.display())))?;
    }
    std::fs::write(&path, contents)
        .map_err(|e| McpError::Internal(format!("write {}: {e}", path.display())))?;
    written.push(rel.to_owned());
    Ok(())
}

/// The guest crate's `Cargo.toml` — a standalone (non-workspace) `wasm32-wasip2`
/// cdylib, mirroring the checked-in `gate-check-example`.
fn guest_cargo_toml(id: &str) -> String {
    format!(
        "# Scaffolded lazybones WASM extension guest.\n\
         #\n\
         # Standalone crate (NOT a workspace member): it targets `wasm32-wasip2`, so the\n\
         # empty `[workspace]` table makes Cargo treat it as its own workspace root.\n\
         # Build:  cargo build --release --target wasm32-wasip2\n\
         [package]\n\
         name = \"{id}\"\n\
         version = \"0.1.0\"\n\
         edition = \"2021\"\n\
         license = \"MIT OR Apache-2.0\"\n\
         publish = false\n\
         \n\
         [workspace]\n\
         \n\
         [lib]\n\
         # A WASI Preview 2 component is produced directly by the `wasm32-wasip2`\n\
         # target from a cdylib.\n\
         crate-type = [\"cdylib\"]\n\
         \n\
         [dependencies]\n\
         # Guest-side Component Model bindings generator. Pin to the version the host\n\
         # workspace tracks so the generated ABI matches.\n\
         wit-bindgen = \"0.58\"\n\
         \n\
         [profile.release]\n\
         opt-level = \"s\"\n\
         strip = true\n\
         lto = true\n"
    )
}

/// The guest `src/lib.rs` skeleton — a `gate-check` implementation when that is (or
/// defaults to) an extension point, else a `todo!()` stub the author fills in.
fn guest_lib_rs(name: &str, description: &str, points: &[String]) -> String {
    let header = format!(
        "//! {name} — {description}\n\
         //!\n\
         //! Scaffolded by `extension.scaffold`. Vendor the lazybones `wit/` world next\n\
         //! to this crate (the host's `wasmtime::component::bindgen!` world), then build:\n\
         //!   cargo build --release --target wasm32-wasip2\n\
         //! and install the produced `.wasm` (loop-only) via `extension.install`.\n\n"
    );

    if points.iter().any(|p| p == DEFAULT_EXTENSION_POINT) {
        format!(
            "{header}\
             wit_bindgen::generate!({{\n\
             \x20   path: \"wit\",\n\
             \x20   world: \"extension\",\n\
             }});\n\
             \n\
             use exports::lazybones::ext::gate_check::{{Guest, GateInput, Verdict, VerdictKind}};\n\
             \n\
             struct Component;\n\
             \n\
             impl Guest for Component {{\n\
             \x20   fn run(input: GateInput) -> Verdict {{\n\
             \x20       // Skeleton policy: skip on an empty diff, otherwise pass. Replace\n\
             \x20       // with the real gate logic.\n\
             \x20       if input.diff.files_changed == 0 {{\n\
             \x20           return Verdict {{\n\
             \x20               kind: VerdictKind::Skip,\n\
             \x20               message: \"no files changed; gate not applicable\".to_string(),\n\
             \x20           }};\n\
             \x20       }}\n\
             \x20       Verdict {{\n\
             \x20           kind: VerdictKind::Pass,\n\
             \x20           message: format!(\"ok: {{}} file(s) changed\", input.diff.files_changed),\n\
             \x20       }}\n\
             \x20   }}\n\
             }}\n\
             \n\
             export!(Component);\n"
        )
    } else {
        let list = points.join(", ");
        format!(
            "{header}\
             // This extension declares the following extension point(s): {list}.\n\
             // Vendor the matching `wit/` world, generate bindings, and implement the\n\
             // exported `Guest` trait(s) here.\n\
             \n\
             wit_bindgen::generate!({{\n\
             \x20   path: \"wit\",\n\
             \x20   world: \"extension\",\n\
             }});\n\
             \n\
             // TODO: implement the exported interface(s) and `export!(Component);`.\n"
        )
    }
}

/// The `lazybones.ext.toml` manifest — kebab-case keys, the source of truth for the
/// extension's identity/caps once embedded as a component custom section (§3.5).
fn manifest_toml(name: &str, points: &[String], caps: &[String], frontend: bool) -> String {
    let points_arr = toml_string_array(points);
    let caps_arr = toml_string_array(caps);
    let mut out = format!(
        "# lazybones extension manifest (embedded into the component on build).\n\
         name = \"{name}\"\n\
         version = \"0.1.0\"\n\
         wit-world = \"extension\"\n\
         extension-points = {points_arr}\n\
         # Capabilities the extension REQUESTS (its declared import surface). Installs\n\
         # are default-deny: the admin grants `\u{2286} requested` separately (loop-only).\n\
         capabilities = {caps_arr}\n"
    );
    if frontend {
        out.push_str(
            "\n[frontend]\n\
             # The federated remote the host imports (design \u{00a7}4).\n\
             entry = \"remoteEntry.js\"\n\
             exposed-module = \"./Extension\"\n\
             sdk-range = \"^1.0\"\n\
             slots = []\n",
        );
    }
    out
}

/// Render a slice of strings as a TOML inline array (`["a", "b"]`).
fn toml_string_array(items: &[String]) -> String {
    let inner = items
        .iter()
        .map(|s| format!("\"{}\"", s.replace('"', "\\\"")))
        .collect::<Vec<_>>()
        .join(", ");
    format!("[{inner}]")
}

/// The scaffold's `README.md` — what was generated + the build/install next steps.
fn readme_md(id: &str, name: &str, points: &[String], frontend: bool) -> String {
    let frontend_note = if frontend {
        "\nA federated frontend remote skeleton is under `frontend/` (see its \
         `vite.config.ts` Module Federation config).\n"
    } else {
        ""
    };
    format!(
        "# {name}\n\n\
         Scaffolded lazybones WASM extension (`{id}`).\n\n\
         Extension point(s): {points}.\n\n\
         ## Build\n\n\
         1. Vendor the lazybones `wit/` world next to this crate.\n\
         2. `cargo build --release --target wasm32-wasip2`\n\n\
         ## Install (loop-only)\n\n\
         Installing a built component and granting it host capabilities is a \
         privileged, loop-only action — an MCP/management agent can author this \
         source but cannot self-install it. Hand the built `.wasm` to the operator, \
         who installs it via `extension.install` (or `POST /extensions`), reviews \
         the requested capabilities, then grants + enables it.\n\
         {frontend_note}",
        points = points.join(", "),
    )
}

/// The frontend remote's `package.json` (Module Federation via Vite).
fn frontend_package_json(id: &str) -> String {
    format!(
        "{{\n\
         \x20 \"name\": \"{id}-frontend\",\n\
         \x20 \"private\": true,\n\
         \x20 \"version\": \"0.1.0\",\n\
         \x20 \"type\": \"module\",\n\
         \x20 \"scripts\": {{\n\
         \x20   \"build\": \"vite build\"\n\
         \x20 }},\n\
         \x20 \"devDependencies\": {{\n\
         \x20   \"@originjs/vite-plugin-federation\": \"^1.3.5\",\n\
         \x20   \"vite\": \"^5\"\n\
         \x20 }}\n\
         }}\n"
    )
}

/// The frontend remote's Vite config exposing `./Extension` as a federated module.
fn frontend_vite_config(id: &str) -> String {
    format!(
        "import {{ defineConfig }} from 'vite';\n\
         import federation from '@originjs/vite-plugin-federation';\n\
         \n\
         // Federated remote skeleton (design \u{00a7}4): the host imports `remoteEntry.js`\n\
         // + the exposed `./Extension` module and mounts it into a UI slot.\n\
         export default defineConfig({{\n\
         \x20 plugins: [\n\
         \x20   federation({{\n\
         \x20     name: '{id}',\n\
         \x20     filename: 'remoteEntry.js',\n\
         \x20     exposes: {{ './Extension': './src/Extension.tsx' }},\n\
         \x20     shared: ['react', 'react-dom'],\n\
         \x20   }}),\n\
         \x20 ],\n\
         \x20 build: {{ target: 'esnext', modulePreload: false, cssCodeSplit: false }},\n\
         }});\n"
    )
}

/// The frontend remote's exposed React component skeleton.
const FRONTEND_EXTENSION_TSX: &str = "// Federated remote entry the lazybones host mounts into a UI slot.\n\
// Replace with the extension's real UI.\n\
export default function Extension() {\n\
  return <div>Scaffolded lazybones extension UI.</div>;\n\
}\n";
