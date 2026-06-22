/** The last path segment of a repo path (basename), for compact display.
 *  The full path goes in a tooltip. Handles trailing slashes and bare names. */
export function repoBasename(repo: string): string {
  const trimmed = repo.replace(/\/+$/, "");
  const idx = trimmed.lastIndexOf("/");
  return idx >= 0 ? trimmed.slice(idx + 1) || trimmed : trimmed;
}
