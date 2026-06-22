import { useState } from "react";
import { Plus, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogTrigger,
} from "@/components/ui/dialog";
import { ApiError } from "@/lib/api/client";
import { useCreateProject } from "@/lib/hooks/use-projects";
import { useTeams } from "@/lib/hooks/use-teams";
import { RepoPicker } from "@/features/workflows/repo-picker";

/** Author a project — id, title, owning team, and the repo target(s) it spans.
 *  Requires `Author` + manager of the owning team (or admin); the backend's role
 *  guard enforces that, the UI only offers it to managers/operators.
 *
 *  Shared between the Projects list and the Team dashboard. `team` pre-selects
 *  (and locks) the owning team when launched from a team context. */
export function CreateProjectDialog({
  team,
  onCreated,
}: {
  team?: string;
  onCreated?: (id: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const [id, setId] = useState("");
  const [title, setTitle] = useState("");
  const [selectedTeam, setSelectedTeam] = useState<string | null>(team ?? null);
  const [repos, setRepos] = useState<string[]>([]);
  const create = useCreateProject();

  function reset() {
    setId("");
    setTitle("");
    setSelectedTeam(team ?? null);
    setRepos([]);
    create.reset();
  }

  function submit() {
    const tid = id.trim();
    if (!tid || !title.trim()) return;
    create.mutate(
      { id: tid, title: title.trim(), team: selectedTeam, repos },
      {
        onSuccess: (p) => {
          setOpen(false);
          reset();
          onCreated?.(p.id);
        },
      },
    );
  }

  const err = create.error;
  const message =
    err instanceof ApiError
      ? err.status === 409
        ? `A project "${id.trim()}" already exists.`
        : err.status === 403
          ? "You need to be a manager of this team (or an admin) to create a project here."
          : err.message
      : err
        ? "Something went wrong."
        : null;

  const canSave = !!id.trim() && !!title.trim() && !create.isPending;

  return (
    <Dialog
      open={open}
      onOpenChange={(o) => {
        setOpen(o);
        if (!o) reset();
      }}
    >
      <DialogTrigger asChild>
        <Button size="sm">
          <Plus /> New project
        </Button>
      </DialogTrigger>
      <DialogContent
        title="New project"
        description="A long-running home for a stream of workflows, owned by a team."
      >
        <div className="space-y-4">
          <Field label="Project id" hint="lowercase, unique, e.g. apollo">
            <Input
              value={id}
              autoFocus
              onChange={(e) => setId(e.target.value)}
              placeholder="apollo"
              className="font-mono"
            />
          </Field>
          <Field label="Title" hint="shown in the project list">
            <Input
              value={title}
              onChange={(e) => setTitle(e.target.value)}
              placeholder="Apollo"
            />
          </Field>
          <Field
            label="Owning team"
            hint="a manager of this team — or an admin — may create it"
          >
            {team ? (
              <Input value={team} disabled className="font-mono" />
            ) : (
              <TeamSelect value={selectedTeam} onChange={setSelectedTeam} />
            )}
          </Field>
          <Field label="Repos" hint="the repo target(s) this project's work spans">
            <RepoList repos={repos} onChange={setRepos} />
          </Field>

          {message && (
            <p className="rounded-md border border-status-blocked/30 bg-status-blocked/10 px-3 py-2 text-xs text-status-blocked">
              {message}
            </p>
          )}

          <div className="flex justify-end gap-2">
            <DialogClose asChild>
              <Button variant="ghost" size="sm">
                Cancel
              </Button>
            </DialogClose>
            <Button size="sm" onClick={submit} disabled={!canSave}>
              Create project
            </Button>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}

/** A repo-list editor backed by the existing host repo-picker. */
function RepoList({
  repos,
  onChange,
}: {
  repos: string[];
  onChange: (repos: string[]) => void;
}) {
  return (
    <div className="space-y-2">
      {repos.length > 0 && (
        <ul className="space-y-1">
          {repos.map((r) => (
            <li
              key={r}
              className="flex items-center justify-between gap-2 rounded-md border border-border bg-surface-2 px-2 py-1"
            >
              <span className="truncate font-mono text-[11px]">{r}</span>
              <button
                type="button"
                onClick={() => onChange(repos.filter((x) => x !== r))}
                className="shrink-0 text-muted-foreground hover:text-foreground"
              >
                <X className="size-3.5" />
              </button>
            </li>
          ))}
        </ul>
      )}
      <RepoPicker
        trigger={
          <Button type="button" variant="secondary" size="sm">
            <Plus /> Add repo
          </Button>
        }
        onPick={(path) => {
          if (!repos.includes(path)) onChange([...repos, path]);
        }}
      />
    </div>
  );
}

/** A team chooser — every team plus a teamless (admin-only) option. */
function TeamSelect({
  value,
  onChange,
}: {
  value: string | null;
  onChange: (team: string | null) => void;
}) {
  const { data: teams } = useTeams();
  return (
    <select
      value={value ?? ""}
      onChange={(e) => onChange(e.target.value || null)}
      className="h-9 w-full rounded-md border border-border bg-surface-2 px-2 text-sm"
    >
      <option value="">— none (admin-only) —</option>
      {(teams ?? []).map((t) => (
        <option key={t.id} value={t.id}>
          {t.title} ({t.id})
        </option>
      ))}
    </select>
  );
}

function Field({
  label,
  hint,
  children,
}: {
  label: string;
  hint?: string;
  children: React.ReactNode;
}) {
  return (
    <label className="block space-y-1">
      <span className="text-xs font-medium">{label}</span>
      {children}
      {hint && <span className="block text-[10px] text-muted-foreground">{hint}</span>}
    </label>
  );
}
