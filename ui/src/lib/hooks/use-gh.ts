import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { listDir } from "@/lib/api/fs";
import {
  checkoutGhBranch,
  closeGhIssue,
  createGhBranch,
  createGhIssue,
  deleteGhBranch,
  getGhAuth,
  getGhRepo,
  listGhBranches,
  listGhIssues,
  listGhLocalBranches,
  listGhWorktrees,
  pruneGhWorktrees,
  removeGhWorktree,
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

/** Refresh everything a branch op can affect: both branch lists, the repo's
 *  current branch, and worktrees (a switch/create moves the main worktree). */
function invalidateBranchState(qc: ReturnType<typeof useQueryClient>, dir: string) {
  qc.invalidateQueries({ queryKey: ["gh-branches", dir] });
  qc.invalidateQueries({ queryKey: ["gh-local-branches", dir] });
  qc.invalidateQueries({ queryKey: ["gh-repo", dir] });
  qc.invalidateQueries({ queryKey: ["gh-worktrees", dir] });
}

/** Make + check out a new branch, then refresh that dir's branch state. */
export function useCreateGhBranch() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ dir, name, from }: { dir: string; name: string; from?: string | null }) =>
      createGhBranch(dir, name, from),
    onSuccess: (_res, { dir }) => invalidateBranchState(qc, dir),
  });
}

/** Local branches for a dir (via `git`, no remote required). */
export function useGhLocalBranches(dir: string | null) {
  return useQuery({
    queryKey: ["gh-local-branches", dir],
    queryFn: ({ signal }) => listGhLocalBranches(dir!, signal),
    enabled: !!dir,
    retry: false,
  });
}

/** Switch to an existing branch, then refresh branches + repo (current branch). */
export function useCheckoutGhBranch() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ dir, branch }: { dir: string; branch: string }) =>
      checkoutGhBranch(dir, branch),
    onSuccess: (_res, { dir }) => invalidateBranchState(qc, dir),
  });
}

/** Delete a local branch (optionally forced), then refresh that dir's branches. */
export function useDeleteGhBranch() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ dir, name, force }: { dir: string; name: string; force?: boolean }) =>
      deleteGhBranch(dir, name, force),
    onSuccess: (_res, { dir }) => invalidateBranchState(qc, dir),
  });
}

/** Worktrees for a dir. */
export function useGhWorktrees(dir: string | null) {
  return useQuery({
    queryKey: ["gh-worktrees", dir],
    queryFn: ({ signal }) => listGhWorktrees(dir!, signal),
    enabled: !!dir,
    retry: false,
  });
}

/** Remove a worktree by path (optionally forced), then refresh the list. */
export function useRemoveGhWorktree() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ dir, path, force }: { dir: string; path: string; force?: boolean }) =>
      removeGhWorktree(dir, path, force),
    onSuccess: (_res, { dir }) =>
      qc.invalidateQueries({ queryKey: ["gh-worktrees", dir] }),
  });
}

/** Prune stale worktree entries, then refresh the list. */
export function usePruneGhWorktrees() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ dir }: { dir: string }) => pruneGhWorktrees(dir),
    onSuccess: (_res, { dir }) =>
      qc.invalidateQueries({ queryKey: ["gh-worktrees", dir] }),
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
