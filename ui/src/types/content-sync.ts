/** Where the local checkout stands vs the remote (mirrors
 *  `lazybones_engine::sync::SyncState`). */
export type SyncState =
  | "unconfigured"
  | "not_checked_out"
  | "synced"
  | "ahead"
  | "behind"
  | "diverged"
  | "unknown";

/** A snapshot of content-sync state (`GET /content-sync/status`). */
export interface SyncStatus {
  state: SyncState;
  /** Commits local is ahead of the remote. */
  ahead: number;
  /** Commits local is behind the remote (drives the pull prompt). */
  behind: number;
  /** Working tree has uncommitted changes. */
  dirty: boolean;
  /** The configured branch. */
  branch: string;
  /** The configured remote URL, if any. */
  remote: string | null;
}

/** The result of running a job (mirrors `lazybones_jobs::JobReport`). */
export interface JobReport {
  job: string;
  summary: string;
}
