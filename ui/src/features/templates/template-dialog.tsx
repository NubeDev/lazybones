import { useState } from "react";
import { Plus } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogTrigger,
  DialogClose,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { cn } from "@/lib/utils/cn";
import { ApiError } from "@/lib/api/client";
import { useCreateTemplate } from "@/lib/hooks/use-templates";
import { WORKTREE_MODES } from "@/features/tasks/worktree-mode";
import { AgentPicker } from "@/features/agents/agent-picker";
import type { TemplateDraft } from "@/lib/api/templates";
import type { WorktreeMode } from "@/types/task";

const EMPTY: TemplateDraft = {
  title: "",
  description: "",
  spec_template: "",
  default_tool: null,
  default_model: null,
  default_effort: null,
  default_worktree_mode: null,
};

/** Author a reusable task template. Surfaces a `409` inline as "id already
 *  exists" rather than crashing. */
export function TemplateDialog({ trigger }: { trigger?: React.ReactNode }) {
  const [open, setOpen] = useState(false);
  const [id, setId] = useState("");
  const [draft, setDraft] = useState<TemplateDraft>(EMPTY);
  const create = useCreateTemplate();

  function reset() {
    setId("");
    setDraft(EMPTY);
  }

  function submit() {
    const trimmed = id.trim();
    if (!trimmed || !draft.title.trim() || !draft.spec_template.trim()) return;
    create.mutate(
      { id: trimmed, draft },
      {
        onSuccess: () => {
          setOpen(false);
          reset();
        },
      },
    );
  }

  const err = create.error;
  const message =
    err instanceof ApiError
      ? err.status === 409
        ? `A template "${id.trim()}" already exists.`
        : err.message
      : err
        ? "Something went wrong."
        : null;

  return (
    <Dialog
      open={open}
      onOpenChange={(o) => {
        setOpen(o);
        if (!o) {
          create.reset();
          reset();
        }
      }}
    >
      <DialogTrigger asChild>
        {trigger ?? (
          <Button size="sm">
            <Plus /> New template
          </Button>
        )}
      </DialogTrigger>
      <DialogContent
        title="New template"
        description="A reusable task recipe. Pick it when adding a task to a workflow."
      >
        <div className="space-y-3">
          <Field label="Template id" hint="lowercase, unique, e.g. open-pr">
            <Input
              value={id}
              autoFocus
              onChange={(e) => setId(e.target.value)}
              placeholder="open-pr"
              className="font-mono"
            />
          </Field>

          <Field label="Title">
            <Input
              value={draft.title}
              onChange={(e) => setDraft({ ...draft, title: e.target.value })}
              placeholder="Open a pull request"
            />
          </Field>

          <Field label="Description" hint="shown in the task picker (optional)">
            <Input
              value={draft.description}
              onChange={(e) => setDraft({ ...draft, description: e.target.value })}
              placeholder="Push the branch and open a PR against base"
            />
          </Field>

          <Field label="Spec template" hint="starting spec text for tasks made from it">
            <textarea
              value={draft.spec_template}
              onChange={(e) =>
                setDraft({ ...draft, spec_template: e.target.value })
              }
              placeholder="The agent's brief: goal, constraints, done-criteria…"
              rows={5}
              className="flex w-full rounded-md border border-border bg-surface-2 px-3 py-2 text-xs leading-relaxed outline-none transition-colors placeholder:text-muted-foreground/70 focus-visible:border-accent/50 focus-visible:ring-2 focus-visible:ring-ring/40 font-mono"
            />
          </Field>

          <AgentPicker
            tool={draft.default_tool ?? ""}
            model={draft.default_model}
            effort={draft.default_effort}
            onToolChange={(t) =>
              setDraft((prev) => ({ ...prev, default_tool: t.trim() || null }))
            }
            onModelChange={(m) => setDraft((prev) => ({ ...prev, default_model: m }))}
            onEffortChange={(e) => setDraft((prev) => ({ ...prev, default_effort: e }))}
            labels={{ agent: "Default agent", agentHint: "blank = inherit run default" }}
          />

          <Field
            label="Default worktree mode"
            hint="rarely set — usually inherits the workspace"
          >
            <ModeSelect
              value={draft.default_worktree_mode}
              onChange={(m) => setDraft({ ...draft, default_worktree_mode: m })}
            />
          </Field>
        </div>

        {message && (
          <p className="mt-3 rounded-md border border-status-blocked/30 bg-status-blocked/10 px-3 py-2 text-xs text-status-blocked">
            {message}
          </p>
        )}

        <div className="mt-4 flex justify-end gap-2">
          <DialogClose asChild>
            <Button variant="ghost" size="sm">
              Cancel
            </Button>
          </DialogClose>
          <Button
            size="sm"
            onClick={submit}
            disabled={
              !id.trim() ||
              !draft.title.trim() ||
              !draft.spec_template.trim() ||
              create.isPending
            }
          >
            Create template
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}

/** A 4-way picker: inherit (null) + the three worktree modes. */
function ModeSelect({
  value,
  onChange,
}: {
  value: WorktreeMode | null;
  onChange: (m: WorktreeMode | null) => void;
}) {
  const options: { key: string; value: WorktreeMode | null; label: string }[] = [
    { key: "inherit", value: null, label: "Inherit" },
    { key: "new", value: "new", label: WORKTREE_MODES.new.label },
    { key: "reuse", value: "reuse", label: WORKTREE_MODES.reuse.label },
    { key: "branch", value: "branch", label: WORKTREE_MODES.branch.label },
  ];
  return (
    <div className="flex gap-1 rounded-md border border-border bg-surface p-0.5">
      {options.map((o) => {
        const on = value === o.value;
        return (
          <button
            key={o.key}
            type="button"
            onClick={() => onChange(o.value)}
            className={cn(
              "flex-1 rounded px-2 py-1 text-[11px] font-medium transition-colors",
              on
                ? "bg-accent-soft/60 text-accent"
                : "text-muted-foreground hover:text-foreground",
            )}
          >
            {o.label}
          </button>
        );
      })}
    </div>
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
