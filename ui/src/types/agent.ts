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

/** Mirror of `lazybones_store::AgentCatalog` — a CRUD-able agent definition
 *  with the models and effort levels it offers. Drives the add-task pickers. */
export interface AgentCatalog {
  /** Tool id — matches the hcom tool key (e.g. `claude`). */
  id: string;
  /** Human label for the UI. */
  label: string;
  /** The env var the CLI reads its credential from. */
  env_var: string;
  /** How to obtain a credential / log in. */
  login_hint: string;
  /** Selectable model ids, most-preferred first; empty = no model picker. */
  models: string[];
  /** Default model when a task names none. */
  default_model: string | null;
  /** Selectable effort levels; empty = no effort picker. */
  efforts: string[];
  /** Default effort when a task names none. */
  default_effort: string | null;
  created_at: string;
  updated_at: string;
}

/** Mirror of `lazybones_api::AgentTestResult` — a live credential probe. */
export interface AgentTestResult {
  tool: string;
  /** The agent authenticated and responded. */
  ok: boolean;
  /** Human-readable outcome (success summary or failure reason). */
  detail: string;
  /** The agent's own reply — model id + identity it reported — when readable. */
  reply?: string | null;
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
