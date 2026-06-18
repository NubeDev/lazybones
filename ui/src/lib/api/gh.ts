import { request } from "./client";
import type {
  GhAuth,
  GhBranch,
  GhIssue,
  GhRepo,
  IssueStateFilter,
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
