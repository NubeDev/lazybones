import { request } from "./client";
import type {
  GhAuth,
  GhBranch,
  GhComment,
  GhIssue,
  GhLocalBranch,
  GhPullRequest,
  GhRepo,
  GhWorktree,
  IssueStateFilter,
  MergeMethod,
  PrStateFilter,
} from "@/types/gh";

/** Every read takes the target repo as `?dir=` (defaults to `.` server-side). */
function dirq(dir: string): string {
  return `?dir=${encodeURIComponent(dir)}`;
}

/** `GET /gh/auth` — is `gh` installed + logged in (unguarded). */
export function getGhAuth(signal?: AbortSignal): Promise<GhAuth> {
  return request<GhAuth>("/gh/auth", { signal });
}

/** `GET /gh/repo?dir=` — repo identity + default branch. */
export function getGhRepo(dir: string, signal?: AbortSignal): Promise<GhRepo> {
  return request<GhRepo>(`/gh/repo${dirq(dir)}`, { auth: true, signal });
}

/** `GET /gh/branches?dir=` — branches to select from. */
export function listGhBranches(dir: string, signal?: AbortSignal): Promise<GhBranch[]> {
  return request<GhBranch[]>(`/gh/branches${dirq(dir)}`, { auth: true, signal });
}

/** `POST /gh/branches` — make + check out a new branch. */
export function createGhBranch(
  dir: string,
  name: string,
  from?: string | null,
): Promise<{ branch: string }> {
  return request<{ branch: string }>("/gh/branches", {
    method: "POST",
    auth: true,
    body: { dir, name, from: from ?? null },
  });
}

/** `GET /gh/local-branches?dir=` — local branches via `git` (no remote needed). */
export function listGhLocalBranches(
  dir: string,
  signal?: AbortSignal,
): Promise<GhLocalBranch[]> {
  return request<GhLocalBranch[]>(`/gh/local-branches${dirq(dir)}`, {
    auth: true,
    signal,
  });
}

/** `POST /gh/checkout` — switch to an existing branch. */
export function checkoutGhBranch(
  dir: string,
  branch: string,
): Promise<{ branch: string }> {
  return request<{ branch: string }>("/gh/checkout", {
    method: "POST",
    auth: true,
    body: { dir, branch },
  });
}

/** `DELETE /gh/branches/:name?dir=&force=` — delete a local branch. */
export function deleteGhBranch(
  dir: string,
  name: string,
  force = false,
): Promise<{ deleted: string }> {
  return request<{ deleted: string }>(
    `/gh/branches/${encodeURIComponent(name)}${dirq(dir)}&force=${force}`,
    { method: "DELETE", auth: true },
  );
}

/** `GET /gh/worktrees?dir=` — list the repo's worktrees. */
export function listGhWorktrees(
  dir: string,
  signal?: AbortSignal,
): Promise<GhWorktree[]> {
  return request<GhWorktree[]>(`/gh/worktrees${dirq(dir)}`, { auth: true, signal });
}

/** `DELETE /gh/worktrees` — remove a worktree by path. */
export function removeGhWorktree(
  dir: string,
  path: string,
  force = false,
): Promise<{ removed: string }> {
  return request<{ removed: string }>("/gh/worktrees", {
    method: "DELETE",
    auth: true,
    body: { dir, path, force },
  });
}

/** `POST /gh/worktrees/prune` — drop stale worktree entries. */
export function pruneGhWorktrees(dir: string): Promise<{ pruned: boolean }> {
  return request<{ pruned: boolean }>("/gh/worktrees/prune", {
    method: "POST",
    auth: true,
    body: { dir },
  });
}

/** `GET /gh/issues?dir=&state=` — list issues. */
export function listGhIssues(
  dir: string,
  state: IssueStateFilter = "open",
  signal?: AbortSignal,
): Promise<GhIssue[]> {
  return request<GhIssue[]>(`/gh/issues${dirq(dir)}&state=${state}`, {
    auth: true,
    signal,
  });
}

/** `GET /gh/issues/:number?dir=` — view one issue. */
export function getGhIssue(
  dir: string,
  number: number,
  signal?: AbortSignal,
): Promise<GhIssue> {
  return request<GhIssue>(`/gh/issues/${number}${dirq(dir)}`, { auth: true, signal });
}

/** `POST /gh/issues` — open a new issue; returns its url. */
export function createGhIssue(
  dir: string,
  title: string,
  body = "",
): Promise<{ url: string }> {
  return request<{ url: string }>("/gh/issues", {
    method: "POST",
    auth: true,
    body: { dir, title, body },
  });
}

/** `POST /gh/issues/:number/close?dir=` — close an issue. */
export function closeGhIssue(dir: string, number: number): Promise<GhIssue> {
  return request<GhIssue>(`/gh/issues/${number}/close${dirq(dir)}`, {
    method: "POST",
    auth: true,
  });
}

/** `GET /gh/mentionable?dir=` — logins that can be `@`-mentioned in the repo. */
export function listGhMentionable(
  dir: string,
  signal?: AbortSignal,
): Promise<string[]> {
  return request<string[]>(`/gh/mentionable${dirq(dir)}`, { auth: true, signal });
}

/** `GET /gh/issues/:number/comments?dir=` — list an issue's comments. */
export function listGhIssueComments(
  dir: string,
  number: number,
  signal?: AbortSignal,
): Promise<GhComment[]> {
  return request<GhComment[]>(`/gh/issues/${number}/comments${dirq(dir)}`, {
    auth: true,
    signal,
  });
}

/** `POST /gh/issues/:number/comments` — add a comment; returns its url. */
export function commentGhIssue(
  dir: string,
  number: number,
  body: string,
): Promise<{ url: string }> {
  return request<{ url: string }>(`/gh/issues/${number}/comments`, {
    method: "POST",
    auth: true,
    body: { dir, body },
  });
}

/** `GET /gh/prs?dir=&state=` — list pull requests. */
export function listGhPrs(
  dir: string,
  state: PrStateFilter = "open",
  signal?: AbortSignal,
): Promise<GhPullRequest[]> {
  return request<GhPullRequest[]>(`/gh/prs${dirq(dir)}&state=${state}`, {
    auth: true,
    signal,
  });
}

/** `POST /gh/prs` — open a new pull request; returns its url. */
export function createGhPr(
  dir: string,
  args: { title: string; body?: string; head: string; base: string; draft?: boolean },
): Promise<{ url: string }> {
  return request<{ url: string }>("/gh/prs", {
    method: "POST",
    auth: true,
    body: {
      dir,
      title: args.title,
      body: args.body ?? "",
      head: args.head,
      base: args.base,
      draft: args.draft ?? false,
    },
  });
}

/** `GET /gh/prs/:number?dir=` — view one pull request. */
export function getGhPr(
  dir: string,
  number: number,
  signal?: AbortSignal,
): Promise<GhPullRequest> {
  return request<GhPullRequest>(`/gh/prs/${number}${dirq(dir)}`, { auth: true, signal });
}

/** `POST /gh/prs/:number/merge` — merge a pull request. */
export function mergeGhPr(
  dir: string,
  number: number,
  method: MergeMethod = "merge",
  deleteBranch = false,
): Promise<GhPullRequest> {
  return request<GhPullRequest>(`/gh/prs/${number}/merge`, {
    method: "POST",
    auth: true,
    body: { dir, method, delete_branch: deleteBranch },
  });
}

/** `POST /gh/prs/:number/close?dir=` — close a PR without merging. */
export function closeGhPr(dir: string, number: number): Promise<GhPullRequest> {
  return request<GhPullRequest>(`/gh/prs/${number}/close${dirq(dir)}`, {
    method: "POST",
    auth: true,
  });
}

/** `GET /gh/prs/:number/comments?dir=` — list a PR's comments. */
export function listGhPrComments(
  dir: string,
  number: number,
  signal?: AbortSignal,
): Promise<GhComment[]> {
  return request<GhComment[]>(`/gh/prs/${number}/comments${dirq(dir)}`, {
    auth: true,
    signal,
  });
}

/** `POST /gh/prs/:number/comments` — add a comment to a PR; returns its url. */
export function commentGhPr(
  dir: string,
  number: number,
  body: string,
): Promise<{ url: string }> {
  return request<{ url: string }>(`/gh/prs/${number}/comments`, {
    method: "POST",
    auth: true,
    body: { dir, body },
  });
}
