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
