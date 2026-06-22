/** How much of the REST surface the agent's scoped token may exercise (mirror of
 *  `lazybones_store::PermissionProfile`). `author_and_manage` lets the agent
 *  *propose* lifecycle actions, each still confirmed by the operator (§10.2). */
export type PermissionProfile = "read_only" | "author" | "author_and_manage";

/** hcom session lifecycle per conversation (mirror of
 *  `lazybones_store::SessionMode`). */
export type SessionMode = "per_conversation" | "per_turn";

/** Mirror of `lazybones_store::ManagementAgentConfig` — the single global
 *  Lazybones-Agent configuration. */
export interface ManagementAgentConfig {
  /** FK into the agent catalog, e.g. `"claude"`. */
  tool: string;
  /** Model ⊆ the tool's catalog entry, or `null` for the CLI default. */
  model: string | null;
  /** Effort ⊆ the tool's catalog entry, or `null` for the CLI default. */
  effort: string | null;
  /** The permission profile bounding the agent's scoped token. */
  permission_profile: PermissionProfile;
  /** hcom session lifecycle per conversation. */
  session_mode: SessionMode;
  /** Skill ids the agent may use as operating runbooks. */
  enabled_skills: string[];
  /** Extra CLI flags for the tool process. */
  permission_flags: string[];
  /** RFC3339 timestamp of the last write. */
  updated_at: string;
}

/** The authored fields of the config (the `PUT` body). */
export interface ManagementAgentDraft {
  tool: string;
  model: string | null;
  effort: string | null;
  permission_profile: PermissionProfile;
  session_mode: SessionMode;
  enabled_skills: string[];
  permission_flags: string[];
}
