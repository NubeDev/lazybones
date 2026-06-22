import { request } from "./client";
import type { DiffResult, FileContent, TreeEntry } from "@/types/files";

/** Every read takes the target repo/worktree as `?dir=`. */
function dirq(dir: string): string {
  return `?dir=${encodeURIComponent(dir)}`;
}

/** `GET /files/tree?dir=&base=` — the whole repo file tree (flat, depth-first),
 *  each file tagged with its git status. `base` set ⇒ also decorate paths the
 *  current branch changed vs that base. */
export function listTree(
  dir: string,
  base: string | null = null,
  signal?: AbortSignal,
): Promise<TreeEntry[]> {
  const q = base ? `${dirq(dir)}&base=${encodeURIComponent(base)}` : dirq(dir);
  return request<TreeEntry[]>(`/files/tree${q}`, { auth: true, signal });
}

/** `GET /files/read?dir=&rel=` — read one file from the working tree. */
export function readFile(
  dir: string,
  rel: string,
  signal?: AbortSignal,
): Promise<FileContent> {
  return request<FileContent>(
    `/files/read${dirq(dir)}&rel=${encodeURIComponent(rel)}`,
    { auth: true, signal },
  );
}

/** `GET /files/diff?dir=&base=&rel=` — unified diff of the working tree.
 *  `base` set ⇒ branch-vs-base; null ⇒ uncommitted changes. `rel` scopes it. */
export function fileDiff(
  dir: string,
  base: string | null = null,
  rel: string | null = null,
  signal?: AbortSignal,
): Promise<DiffResult> {
  const params = new URLSearchParams({ dir });
  if (base) params.set("base", base);
  if (rel) params.set("rel", rel);
  return request<DiffResult>(`/files/diff?${params.toString()}`, {
    auth: true,
    signal,
  });
}
