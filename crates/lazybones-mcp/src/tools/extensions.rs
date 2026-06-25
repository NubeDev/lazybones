//! Extension tools — author vs. install, the sharp §6.3 split.
//!
//! `extension.scaffold` generates guest source + `lazybones.ext.toml` manifest into
//! a repo/worktree — a *file-writing* act, gated by `Author`/`Document`. Reads
//! (`extension.list`/`get`) need none. `extension.install`/`set_grants`/`enable`/
//! `disable`/`invoke` require **loop-only** `Capability::Extension` — installing
//! sandboxed code and granting it host capabilities is the single most privileged
//! act on the surface, so an MCP agent can author an extension but never
//! self-install + self-grant it (extension-system §3.3 trust boundary).
//!
//! Scaffold: no tools yet (task `mcp-crate`); the §6.3 set lands in `mcp-extensions`.
