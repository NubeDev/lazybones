import { useState } from "react";
import { ChevronUp, Folder, FolderGit2, Check } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogTrigger,
  DialogClose,
} from "@/components/ui/dialog";
import { ApiError } from "@/lib/api/client";
import { useDirListing } from "@/lib/hooks/use-gh";

/** A native-feeling host directory browser for choosing the workflow's repo.
 *  Walks the filesystem via `GET /fs/list`, flags git repos, and returns the
 *  chosen absolute path. Starts at `$HOME` (path = null). */
export function RepoPicker({
  trigger,
  onPick,
}: {
  trigger: React.ReactNode;
  onPick: (path: string) => void;
}) {
  const [open, setOpen] = useState(false);
  // null ⇒ server resolves $HOME; thereafter we track the absolute path.
  const [path, setPath] = useState<string | null>(null);
  const { data, error, isLoading } = useDirListing(path, open);

  function choose(p: string) {
    onPick(p);
    setOpen(false);
  }

  const message =
    error instanceof ApiError ? error.message : error ? "Cannot read directory." : null;

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger asChild>{trigger}</DialogTrigger>
      <DialogContent
        title="Choose a repo"
        description="Browse to a git repository on this machine."
      >
        {/* Current location + up. */}
        <div className="flex items-center gap-2">
          <Button
            variant="ghost"
            size="sm"
            disabled={!data?.parent}
            onClick={() => data?.parent && setPath(data.parent)}
            title="Up one level"
          >
            <ChevronUp />
          </Button>
          <code className="flex-1 truncate rounded-md border border-border bg-surface-2/40 px-2 py-1 text-xs">
            {data?.path ?? "…"}
          </code>
          {data?.is_repo && (
            <Button size="sm" onClick={() => choose(data.path)}>
              <Check /> Use this
            </Button>
          )}
        </div>

        <div className="mt-3 max-h-72 overflow-auto rounded-md border border-border">
          {isLoading && (
            <p className="px-3 py-4 text-xs text-muted-foreground">Loading…</p>
          )}
          {message && (
            <p className="px-3 py-4 text-xs text-status-blocked">{message}</p>
          )}
          {data && data.entries.length === 0 && !isLoading && (
            <p className="px-3 py-4 text-xs text-muted-foreground">
              No subdirectories here.
            </p>
          )}
          <ul className="divide-y divide-border">
            {data?.entries.map((e) => (
              <li key={e.path}>
                <div className="flex items-center gap-2 px-3 py-1.5 hover:bg-surface-2/40">
                  <button
                    className="flex flex-1 items-center gap-2 text-left text-sm"
                    onClick={() => setPath(e.path)}
                  >
                    {e.is_repo ? (
                      <FolderGit2 className="size-4 text-accent" />
                    ) : (
                      <Folder className="size-4 text-muted-foreground" />
                    )}
                    <span className="truncate">{e.name}</span>
                  </button>
                  {e.is_repo && (
                    <Button variant="ghost" size="sm" onClick={() => choose(e.path)}>
                      Select
                    </Button>
                  )}
                </div>
              </li>
            ))}
          </ul>
        </div>

        <div className="mt-4 flex justify-end">
          <DialogClose asChild>
            <Button variant="ghost" size="sm">
              Cancel
            </Button>
          </DialogClose>
        </div>
      </DialogContent>
    </Dialog>
  );
}
