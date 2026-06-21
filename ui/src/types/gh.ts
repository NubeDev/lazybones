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

/** One local branch (`GET /gh/local-branches`) — from `git`, no remote needed. */
export interface GhLocalBranch {
  name: string;
  sha: string;
  /** Upstream tracking ref (e.g. `origin/master`), if any. */
  upstream: string | null;
  ahead: number;
  behind: number;
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

/** One comment on an issue (`GET /gh/issues/:number/comments`). */
export interface GhComment {
  author: string | null;
  body: string;
  url: string;
  created_at: string | null;
}

/** Which pull requests to list. */
export type PrStateFilter = "open" | "closed" | "merged" | "all";

/** How to merge a PR (mirrors `gh pr merge` strategies). */
export type MergeMethod = "merge" | "squash" | "rebase";

/** One GitHub pull request (`GET /gh/prs`). */
export interface GhPullRequest {
  number: number;
  title: string;
  /** `OPEN` / `CLOSED` / `MERGED`. */
  state: string;
  url: string;
  body: string;
  author: string | null;
  labels: string[];
  /** Source branch. */
  head_ref: string;
  /** Target branch. */
  base_ref: string;
  is_draft: boolean;
  /** `MERGEABLE` / `CONFLICTING` / `UNKNOWN`. */
  mergeable: string;
  /** RFC3339 timestamps. `closed_at`/`merged_at` are null while open. */
  created_at: string | null;
  updated_at: string | null;
  closed_at: string | null;
  merged_at: string | null;
}
