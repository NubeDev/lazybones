//! `POST /mcp/token` — mint a profile-scoped management token for an external
//! MCP client (`docs/mcp/README.md` §9 OQ1).
//!
//! The MCP surface authenticates exactly like a REST request: a bearer token
//! resolved to a [`ScopedSession`](lazybones_auth::ScopedSession). This route lets
//! an operator mint such a token for an out-of-app client (Claude Desktop, the
//! `claude` CLI, Cursor, …) to put in its `Authorization` header. It reuses
//! [`AppState::mint_management_token`] — the *same* minting path the in-app agent
//! uses — so the token is a strict subset of the loop's grant and **never** carries
//! `Claim`/`Secret`/`Extension`. No new privilege is created here: this is just a
//! second way to obtain an existing management profile's token.
//!
//! Minting itself is guarded by `Capability::Author` (only an operator holding the
//! loop/author grant may hand out tokens), mirroring the management-agent config
//! writer.

use axum::Json;
use axum::extract::State;
use lazybones_auth::{Capability, ManagementProfile};
use lazybones_store::PermissionProfile;
use serde::Serialize;

use crate::dto::MintMcpTokenBody;
use crate::error::ApiResult;
use crate::extract::Session;
use crate::state::AppState;

/// The minted bearer plus the context an external client needs to register: the
/// `/mcp` endpoint URL and the resolved profile (so the UI can echo what was
/// granted).
#[derive(Debug, Serialize)]
pub struct MintMcpTokenResponse {
    /// The bearer string to put in the client's `Authorization: Bearer …` header.
    pub token: String,
    /// The resolved profile the token carries (the lowercase wire form).
    pub profile: String,
    /// The in-process MCP endpoint this token authenticates against.
    pub mcp_url: String,
}

/// `POST /mcp/token` — mint a management token for an external MCP client.
/// Requires `Author` (only an operator may issue tokens). Returns the bearer,
/// resolved profile, and the `/mcp` URL to register against.
pub async fn mint_mcp_token(
    State(state): State<AppState>,
    session: Session,
    Json(body): Json<MintMcpTokenBody>,
) -> ApiResult<Json<MintMcpTokenResponse>> {
    session.require(Capability::Author, "author", "mcp-token")?;

    let profile = PermissionProfile::parse(&body.profile);
    let management = auth_profile(profile);

    // Fold an optional human label into the actor for auditability; sanitise it so
    // the generated token stays a clean `lazybones-agent-<actor>-<n>` string.
    let label = body.label.as_deref().map(sanitize_label);
    let actor = match label {
        Some(label) if !label.is_empty() => format!("mcp-{label}"),
        _ => "mcp-client".to_owned(),
    };

    let token = state.mint_management_token(&actor, management);
    let mcp_url = format!("{}/mcp", state.base_url.trim_end_matches('/'));

    Ok(Json(MintMcpTokenResponse {
        token,
        profile: profile.as_str().to_owned(),
        mcp_url,
    }))
}

/// Project the store's permission profile into the auth crate's capability profile
/// (the API layer owns this seam so the store needs no auth dependency). Mirrors
/// `agent_chat::auth_profile`.
fn auth_profile(profile: PermissionProfile) -> ManagementProfile {
    match profile {
        PermissionProfile::ReadOnly => ManagementProfile::ReadOnly,
        PermissionProfile::Author => ManagementProfile::Author,
        PermissionProfile::AuthorAndManage => ManagementProfile::AuthorAndManage,
    }
}

/// Keep only token-safe characters (`[a-z0-9-]`) from a client label, lowercasing
/// the rest so an operator-supplied tag can't break the token format.
fn sanitize_label(label: &str) -> String {
    label
        .trim()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_owned()
}
