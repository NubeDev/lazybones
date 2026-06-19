/** Mirrors of the `/fs/*` and `/gh/*` REST DTOs (crates/lazybones-api). */

/** One browsable child directory in the repo/dir picker (`GET /fs/list`). */
export interface DirEntry {
  name: string;
  path: string;
  /** Whether this dir is itself a git repo (has a `.git`). */
  is_repo: boolean;
}

/** A directory listing: where we are, where "up" goes, and the children. */
export interface FsListing {
  path: string;
  parent: string | null;
  is_repo: boolean;
  entries: DirEntry[];
}

/** `GET /gh/auth` — is the user's `gh` CLI installed + logged in. */
export interface GhAuth {
  authenticated: boolean;
  detail: string | null;
}

/** `GET /gh/repo` — repo identity + default branch. */
export interface GhRepo {
  full_name: string;
  name: string;
  owner: string;
  url: string;
  description: string;
  default_branch: string | null;
}

/** One branch in the selector (`GET /gh/branches`). */
export interface GhBranch {
  name: string;
  sha: string;
  protected: boolean;
}

/** One worktree row (`GET /gh/worktrees`). */
export interface GhWorktree {
  path: string;
  branch: string | null;
  /** Tip commit SHA; `null` for a bare/unborn entry. */
  head: string | null;
  /** The repo's primary checkout (never removable from the UI). */
  is_main: boolean;
  locked: boolean;
}

/** Which issues to list. */
export type IssueStateFilter = "open" | "closed" | "all";

/** One GitHub issue (`GET /gh/issues`). */
export interface GhIssue {
  number: number;
  title: string;
  state: string;
  url: string;
  body: string;
  author: string | null;
  labels: string[];
}
