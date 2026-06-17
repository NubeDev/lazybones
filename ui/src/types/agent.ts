/** Mirror of `lazybones_api::EngineReport` — hcom availability. */
export interface EngineReport {
  engine: string;
  installed: boolean;
  version: string | null;
  install_hint: string;
}

/** Mirror of `lazybones_api::AgentReport` — one agent CLI's setup state. */
export interface AgentReport {
  tool: string;
  label: string;
  installed: boolean;
  version: string | null;
  env_var: string;
  /** A credential is stored for this tool in the secret store. */
  key_stored: boolean;
  /** The env var is already present in the daemon's environment. */
  key_in_env: boolean;
  /** Installed AND has a credential — ready to run a task. */
  ready: boolean;
  login_hint: string;
}

/** Mirror of `lazybones_api::AgentTestResult` — a live credential probe. */
export interface AgentTestResult {
  tool: string;
  /** The agent authenticated and responded. */
  ok: boolean;
  /** Human-readable outcome (success summary or failure reason). */
  detail: string;
}

/** Mirror of `lazybones_store::SecretMeta` — a stored credential (no value). */
export interface SecretMeta {
  tool: string;
  env_var: string;
  set: boolean;
  /** `…last4` of the value, for confirmation. */
  hint: string;
  updated_at: string;
}
