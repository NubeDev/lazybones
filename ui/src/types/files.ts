/** Mirrors of the `/files/*` REST DTOs (crates/lazybones-api/src/routes/files.rs). */

/** A single-letter git change tag for a file in the tree. */
export type FileStatus = "M" | "A" | "D" | "U";

/** One node of the repo's full file tree (`GET /files/tree`). The endpoint
 *  returns every dir + file as a flat, depth-first-ordered list. */
export interface TreeEntry {
  name: string;
  /** Repo-relative path; the read/diff target for files. */
  path: string;
  is_dir: boolean;
  /** Git change tag for a changed file; null when unchanged (or a dir). */
  status: FileStatus | null;
}

/** A file's contents for the viewer (`GET /files/read`). */
export interface FileContent {
  path: string;
  content: string;
  /** True when the bytes aren't valid text — show "binary file" instead. */
  binary: boolean;
}

/** A unified diff plus the base it was computed against (`GET /files/diff`). */
export interface DiffResult {
  /** `git diff` unified output; empty string = no changes. */
  diff: string;
  /** Base branch, when this is a branch-vs-base diff; null = uncommitted. */
  base: string | null;
}
