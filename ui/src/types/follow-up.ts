/** Mirror of `lazybones_store::FollowUp` — a durable "a human needs to act" note.
 *  Filed by the scheduler when it hits a wall it can't clear (a consent screen, a
 *  spawn failure, a missing credential) or by an agent flagging something; cleared
 *  by an operator. */
export interface FollowUp {
  /** Opaque row id — the resolve handle. */
  id: string;
  /** The run (workflow `run_id`) this follow-up belongs to. */
  run: string;
  /** The task it concerns, if any. */
  task: string | null;
  /** Coarse class the UI groups by. */
  kind: string;
  /** One-line summary. */
  title: string;
  /** Full reason + suggested fix (markdown). */
  detail: string;
  /** Who filed it (`scheduler:tick`, `scheduler:finish`, or an agent session). */
  actor: string;
  /** `open` until resolved. */
  status: "open" | "resolved";
  /** How many times this exact wall was hit — a stuck loop's pressure gauge. */
  seen: number;
  created_at: string;
  updated_at: string;
  resolved_at: string | null;
}
