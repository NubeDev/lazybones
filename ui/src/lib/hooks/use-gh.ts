import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { listDir } from "@/lib/api/fs";
import {
  closeGhIssue,
  createGhBranch,
  createGhIssue,
  getGhAuth,
  getGhRepo,
  listGhBranches,
  listGhIssues,
} from "@/lib/api/gh";
import type { IssueStateFilter } from "@/types/gh";

/** Browse host directories for the repo/dir picker. `path = null` ⇒ `$HOME`. */
export function useDirListing(path: string | null, enabled = true) {
  return useQuery({
    queryKey: ["fs", path],
    queryFn: ({ signal }) => listDir(path ?? undefined, signal),
    enabled,
    retry: false,
  });
}

/** Is the user's `gh` CLI installed + logged in (slow-changing). */
export function useGhAuth() {
  return useQuery({
    queryKey: ["gh-auth"],
    queryFn: ({ signal }) => getGhAuth(signal),
    refetchInterval: 30000,
    retry: false,
  });
}

/** Repo identity + default branch for a chosen dir. Skipped until `dir` is set. */
export function useGhRepo(dir: string | null) {
  return useQuery({
    queryKey: ["gh-repo", dir],
    queryFn: ({ signal }) => getGhRepo(dir!, signal),
    enabled: !!dir,
    retry: false,
  });
}

/** Branches for a chosen dir, for the branch selector. */
export function useGhBranches(dir: string | null) {
  return useQuery({
    queryKey: ["gh-branches", dir],
    queryFn: ({ signal }) => listGhBranches(dir!, signal),
    enabled: !!dir,
    retry: false,
  });
}

/** Make + check out a new branch, then refresh that dir's branch list. */
export function useCreateGhBranch() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ dir, name, from }: { dir: string; name: string; from?: string | null }) =>
      createGhBranch(dir, name, from),
    onSuccess: (_res, { dir }) =>
      qc.invalidateQueries({ queryKey: ["gh-branches", dir] }),
  });
}

/** Issues for a dir, filtered by state. */
export function useGhIssues(dir: string | null, state: IssueStateFilter = "open") {
  return useQuery({
    queryKey: ["gh-issues", dir, state],
    queryFn: ({ signal }) => listGhIssues(dir!, state, signal),
    enabled: !!dir,
    retry: false,
  });
}

/** Open a new issue, then refresh that dir's issue lists. */
export function useCreateGhIssue() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ dir, title, body }: { dir: string; title: string; body?: string }) =>
      createGhIssue(dir, title, body),
    onSuccess: (_res, { dir }) =>
      qc.invalidateQueries({ queryKey: ["gh-issues", dir] }),
  });
}

/** Close an issue, then refresh that dir's issue lists. */
export function useCloseGhIssue() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ dir, number }: { dir: string; number: number }) =>
      closeGhIssue(dir, number),
    onSuccess: (_res, { dir }) =>
      qc.invalidateQueries({ queryKey: ["gh-issues", dir] }),
  });
}
