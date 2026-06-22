import { useState } from "react";
import { GitBranch, Plus } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { ApiError } from "@/lib/api/client";
import { useGhBranches, useCreateGhBranch } from "@/lib/hooks/use-gh";

/** Pick the workflow's base branch from the repo's existing branches, or create
 *  a new one off the current default. Needs the chosen repo `dir`; until one is
 *  set it degrades to a plain text input so the form still works offline / for
 *  non-GitHub repos. Empty value ⇒ inherit the global default (server-side). */
export function BranchField({
  dir,
  value,
  onChange,
}: {
  dir: string | null;
  value: string | null;
  onChange: (branch: string | null) => void;
}) {
  const { data: branches, error, isLoading } = useGhBranches(dir);
  const create = useCreateGhBranch();
  const [creating, setCreating] = useState(false);
  const [newName, setNewName] = useState("");

  // No repo chosen yet, or branch listing failed (e.g. not a gh remote): fall
  // back to a free-text branch name so the field is never a dead end.
  const unavailable = !dir || error instanceof ApiError;
  if (unavailable) {
    return (
      <Input
        value={value ?? ""}
        onChange={(e) => onChange(e.target.value.trim() || null)}
        placeholder="main"
        className="font-mono"
      />
    );
  }

  function submitNew() {
    const name = newName.trim();
    if (!name || !dir) return;
    create.mutate(
      { dir, name },
      {
        onSuccess: ({ branch }) => {
          onChange(branch);
          setCreating(false);
          setNewName("");
        },
      },
    );
  }

  if (creating) {
    return (
      <div className="space-y-1">
        <div className="flex gap-2">
          <Input
            value={newName}
            autoFocus
            onChange={(e) => setNewName(e.target.value)}
            placeholder="feat/new-branch"
            className="font-mono"
          />
          <Button size="sm" onClick={submitNew} disabled={!newName.trim() || create.isPending}>
            Create
          </Button>
          <Button variant="ghost" size="sm" onClick={() => setCreating(false)}>
            Cancel
          </Button>
        </div>
        {create.error && (
          <span className="block text-[10px] text-status-blocked">
            {create.error instanceof ApiError ? create.error.message : "Could not create branch."}
          </span>
        )}
      </div>
    );
  }

  return (
    <div className="flex gap-2">
      <div className="relative flex-1">
        <GitBranch className="pointer-events-none absolute left-2 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground" />
        <select
          value={value ?? ""}
          onChange={(e) => onChange(e.target.value || null)}
          disabled={isLoading}
          className="h-8 w-full rounded-md border border-border bg-surface pl-7 pr-2 font-mono text-xs outline-none focus-visible:ring-2 focus-visible:ring-ring/70"
        >
          <option value="">{isLoading ? "Loading…" : "inherit default"}</option>
          {branches?.map((b) => (
            <option key={b.name} value={b.name}>
              {b.name}
              {b.protected ? " (protected)" : ""}
            </option>
          ))}
        </select>
      </div>
      <Button variant="ghost" size="sm" onClick={() => setCreating(true)} title="New branch">
        <Plus /> New
      </Button>
    </div>
  );
}
