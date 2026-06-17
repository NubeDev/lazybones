//! The catalog of agent CLIs lazybones knows how to run.
//!
//! Each entry maps a `tool` id to the env var that authenticates it and how a
//! user logs in. The `tool` id matches the key hcom reports under `tools` in
//! `hcom status --json` — that is, hcom is the source of truth for *which* tools
//! exist and whether they're installed; this catalog only adds the credential
//! metadata (env var + login hint) hcom doesn't carry. The `GET /agents` route
//! joins the two against the stored-secret set to tell the UI what's set up.

/// One known agent tool and how it authenticates.
pub(crate) struct AgentSpec {
    /// The `agent_tool` id — must match the hcom `tools` key (`hcom status --json`).
    pub tool: &'static str,
    /// Human label for the UI.
    pub label: &'static str,
    /// The env var the CLI reads its credential from.
    pub env_var: &'static str,
    /// How to obtain a credential / log in (shown as a hint in the UI).
    pub login_hint: &'static str,
}

/// The agent CLIs lazybones surfaces in the credentials panel. The id of each
/// entry is an hcom tool key; hcom (`hcom [N] claude|codex|gemini|opencode|kilo|
/// pi|agy|cursor-agent|kimi|copilot`) decides what's installed, and we layer the
/// credential metadata on top. Keep ids in sync with hcom's `tools` map.
pub(crate) const AGENTS: &[AgentSpec] = &[
    AgentSpec {
        tool: "claude",
        label: "Claude Code",
        env_var: "ANTHROPIC_API_KEY",
        login_hint: "console.anthropic.com/settings/keys — or run `claude login`",
    },
    AgentSpec {
        tool: "codex",
        label: "Codex",
        env_var: "OPENAI_API_KEY",
        login_hint: "platform.openai.com/api-keys — or run `codex login`",
    },
    AgentSpec {
        tool: "gemini",
        label: "Gemini CLI",
        env_var: "GEMINI_API_KEY",
        login_hint: "aistudio.google.com/apikey",
    },
    AgentSpec {
        tool: "copilot",
        label: "GitHub Copilot",
        env_var: "GITHUB_TOKEN",
        login_hint: "run `copilot` and sign in, or set a GitHub token",
    },
    AgentSpec {
        tool: "opencode",
        label: "OpenCode",
        env_var: "OPENCODE_API_KEY",
        login_hint: "provider-agnostic — set the key for its configured model",
    },
    AgentSpec {
        tool: "cursor",
        label: "Cursor Agent",
        env_var: "CURSOR_API_KEY",
        login_hint: "run `cursor-agent login`",
    },
    AgentSpec {
        tool: "kilo",
        label: "Kilo Code",
        env_var: "OPENROUTER_API_KEY",
        login_hint: "openrouter.ai/keys — provider key for the configured model",
    },
    AgentSpec {
        tool: "pi",
        label: "Pi",
        env_var: "PI_API_KEY",
        login_hint: "set the API key for Pi's configured provider",
    },
    AgentSpec {
        tool: "kimi",
        label: "Kimi",
        env_var: "MOONSHOT_API_KEY",
        login_hint: "platform.moonshot.cn — Moonshot/Kimi API key",
    },
    AgentSpec {
        tool: "antigravity",
        label: "Antigravity",
        env_var: "GEMINI_API_KEY",
        login_hint: "aistudio.google.com/apikey",
    },
];
