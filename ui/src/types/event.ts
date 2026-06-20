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
