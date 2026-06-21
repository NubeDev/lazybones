/** The single global user-preferences record (mirrors `lazybones_store::Preferences`).
 *  Operator UI choices that follow the operator across browsers/devices. */
export interface Preferences {
  /** IANA timezone name, or `null` to follow the browser's zone. */
  timezone: string | null;
  /** UI theme: `"light" | "dark" | "system"`, or `null` for system. */
  theme: string | null;
  /** RFC3339 timestamp of the last write (empty until first saved). */
  updated_at: string;
}

/** `PUT /settings/preferences` body. An omitted/empty field clears that
 *  preference (reverts to its default). */
export interface PreferencesDraft {
  timezone?: string | null;
  theme?: string | null;
}
