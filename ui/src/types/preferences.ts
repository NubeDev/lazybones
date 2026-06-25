/** Content-sync configuration (mirrors `lazybones_store::SyncConfig`). The
 *  git-backed sync repo that carries authored docs/skills/tasks/templates/
 *  workflows between machines. */
export interface SyncConfig {
  /** Master switch for the automatic behaviours (auto-pull on boot + periodic
   *  auto-push). Manual pull/push still work when this is off. */
  enabled: boolean;
  /** Git remote URL of the cloud sync repo, or `null`/empty => not configured. */
  remote: string | null;
  /** Branch to sync on; `null` is treated as `"main"`. */
  branch: string | null;
  /** Absolute path to the local checkout; `null` lets the daemon derive one. */
  dir: string | null;
  /** Push the export automatically after store changes. */
  auto_push: boolean;
  /** Pull + import automatically on daemon boot. */
  auto_pull: boolean;
}

/** The single global user-preferences record (mirrors `lazybones_store::Preferences`).
 *  Operator UI choices that follow the operator across browsers/devices. */
export interface Preferences {
  /** IANA timezone name, or `null` to follow the browser's zone. */
  timezone: string | null;
  /** UI theme: `"light" | "dark" | "system"`, or `null` for system. */
  theme: string | null;
  /** Content-sync config, or `null` until the operator sets it up. */
  sync: SyncConfig | null;
  /** RFC3339 timestamp of the last write (empty until first saved). */
  updated_at: string;
}

/** `PUT /settings/preferences` body. An omitted/empty field clears that
 *  preference (reverts to its default). */
export interface PreferencesDraft {
  timezone?: string | null;
  theme?: string | null;
  sync?: SyncConfig | null;
}
