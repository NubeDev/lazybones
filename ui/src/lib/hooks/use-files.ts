import { useQuery } from "@tanstack/react-query";
import { fileDiff, listTree, readFile } from "@/lib/api/files";

/** The whole repo file tree (flat, depth-first), each file git-status tagged.
 *  `base` decorates branch-vs-base changes too. Disabled until `dir` is set. */
export function useFileTree(dir: string | null, base: string | null = null) {
  return useQuery({
    queryKey: ["files-tree", dir, base],
    queryFn: ({ signal }) => listTree(dir!, base, signal),
    enabled: !!dir,
    retry: false,
  });
}

/** Contents of one repo-relative file. Disabled until both `dir` and `rel`. */
export function useFileContent(dir: string | null, rel: string | null) {
  return useQuery({
    queryKey: ["file-content", dir, rel],
    queryFn: ({ signal }) => readFile(dir!, rel!, signal),
    enabled: !!dir && !!rel,
    retry: false,
  });
}

/** Unified diff of the working tree. `base = null` ⇒ uncommitted changes;
 *  a branch name ⇒ branch-vs-base. `rel` (optional) scopes to one path. */
export function useFileDiff(
  dir: string | null,
  base: string | null,
  rel: string | null = null,
) {
  return useQuery({
    queryKey: ["file-diff", dir, base, rel],
    queryFn: ({ signal }) => fileDiff(dir!, base, rel, signal),
    enabled: !!dir,
    retry: false,
  });
}
