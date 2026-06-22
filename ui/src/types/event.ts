/** Mirror of `lazybones_store::Event` — one run-log transition row. */
export interface RunEvent {
  run: string;
  task: string;
  from: string;
  to: string;
  actor: string;
  /** RFC3339 timestamp of the transition. */
  at: string;
}

/** The kind of hcom event recorded in the log. */
export type HcomLogKind = "message" | "status" | "life";

/** Mirror of `lazybones_store::HcomLogEntry` — one ingested hcom event.
 *  Streamed live over `/stream` as a `hcom_log` named SSE event and served
 *  durably from `GET /runs/:id/hcom`. */
export interface HcomLogEntry {
  run: string;
  /** The resolved task id this event belongs to, or null if unattributed. */
  task: string | null;
  agent: string;
  tag: string | null;
  hcom_id: number;
  kind: HcomLogKind;
  /** Opaque payload; a `message` typically carries `{ text, ... }` and may set
   *  `truncated: true` when capped at 64 KiB. */
  data: unknown;
  /** RFC3339 timestamp of the event. */
  at: string;
}

/** One entry of hcom's `transcript … --json --full` output: a step of the agent's
 *  run with its narration, the files it touched, and a timestamp. Fields are
 *  optional because hcom's shape can vary across tools/versions. */
export interface TranscriptEntry {
  /** The agent's narration / reasoning text for this step. */
  action?: string;
  /** Files referenced or edited in this step. */
  files?: string[];
  /** 1-based step index within the run. */
  position?: number;
  /** RFC3339 timestamp of the step. */
  timestamp?: string;
  /** The prompt/user text in view at this step (usually only the first entry). */
  user?: string;
}
