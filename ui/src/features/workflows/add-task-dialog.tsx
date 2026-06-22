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
import { useAddWorkflowTask } from "@/lib/hooks/use-workflows";
import { useTemplates } from "@/lib/hooks/use-templates";
import { useAgentCatalog } from "@/lib/hooks/use-agent-catalog";
import { AgentPicker } from "@/features/agents/agent-picker";
import { WORKTREE_MODES } from "@/features/tasks/worktree-mode";
import type { WorkflowTaskDraft } from "@/lib/api/workflows";
import type { WorktreeMode } from "@/types/task";
import type { WorkflowDetail } from "@/types/workflow";

/** Add a task to a workflow. Spec comes from a template (when chosen) or is typed
 *  inline. `reuse_from` only appears when the effective worktree mode is `reuse`
 *  (override, else the workspace default). Existing tasks supply the dep picker
 *  and the reuse-from picker — both keyed off ids within this workflow. */
export function AddTaskDialog({
  workflow,
  existingTasks,
  trigger,
}: {
  workflow: WorkflowDetail;
  /** Task ids already in this workflow, for dep + reuse_from pickers. */
  existingTasks: string[];
  trigger?: React.ReactNode;
}) {
  const { data: templates } = useTemplates();
  const { data: agents } = useAgentCatalog();
  const add = useAddWorkflowTask(workflow.id);

  const [open, setOpen] = useState(false);
  const [id, setId] = useState("");
  const [title, setTitle] = useState("");
  const [spec, setSpec] = useState("");
  const [fromTemplate, setFromTemplate] = useState<string | null>(null);
  const [deps, setDeps] = useState<string[]>([]);
  const [owns, setOwns] = useState("");
  const [tool, setTool] = useState("");
  const [model, setModel] = useState<string | null>(null);
  const [effort, setEffort] = useState<string | null>(null);
  const [override, setOverride] = useState<WorktreeMode | null>(null);
  const [reuseFrom, setReuseFrom] = useState<string | null>(null);

  // The catalog entry for the chosen tool (if it's a known agent) — drives the
  // model + effort pickers. Blank tool / unknown id → no pickers, just the input.
  const agent = (agents ?? []).find((a) => a.id === tool.trim()) ?? null;

  // Effective mode: a non-null override wins; else the workspace's mode.
  const effectiveMode: WorktreeMode = override ?? workflow.workspace.worktree_mode;
  const showReuse = effectiveMode === "reuse";

  // A reuse source that's a task in THIS workflow is also a dependency: the
  // backend folds it into `deps` so the source finishes (and has a worktree)
  // before this task claims it. A source typed in by hand may live in another
  // workflow — then it's not an in-graph dep, just a claim-time wait.
  const reuseIsInWorkflow = reuseFrom !== null && existingTasks.includes(reuseFrom);
  const reuseImpliesDep = showReuse && reuseIsInWorkflow && !deps.includes(reuseFrom!);

  const pickedTemplate = templates?.find((t) => t.id === fromTemplate) ?? null;

  function reset() {
    setId("");
    setTitle("");
    setSpec("");
    setFromTemplate(null);
    setDeps([]);
    setOwns("");
    setTool("");
    setModel(null);
    setEffort(null);
    setOverride(null);
    setReuseFrom(null);
  }

  function toggleDep(d: string) {
    setDeps((cur) => (cur.includes(d) ? cur.filter((x) => x !== d) : [...cur, d]));
  }

  function submit() {
    const tid = id.trim();
    if (!tid || !title.trim()) return;
    const draft: WorkflowTaskDraft = {
      id: tid,
      title: title.trim(),
      deps,
      owns: owns
        .split(",")
        .map((s) => s.trim())
        .filter(Boolean),
      tool: tool.trim() || null,
      // Only send model/effort the chosen agent actually offers.
      model: agent?.models.includes(model ?? "") ? model : null,
      effort: agent?.efforts.includes(effort ?? "") ? effort : null,
      worktree_mode_override: override,
      reuse_from: showReuse ? reuseFrom : null,
    };
    if (fromTemplate) draft.from_template = fromTemplate;
    else draft.spec = spec;

    add.mutate(draft, {
      onSuccess: () => {
        setOpen(false);
        reset();
      },
    });
  }

  const err = add.error;
  const message =
    err instanceof ApiError
      ? err.status === 409
        ? `A task "${id.trim()}" already exists (or the template is missing).`
        : err.status === 404
          ? "This workflow no longer exists."
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
          add.reset();
          reset();
        }
      }}
    >
      <DialogTrigger asChild>
        {trigger ?? (
          <Button size="sm">
            <Plus /> Add task
          </Button>
        )}
      </DialogTrigger>
      <DialogContent
        title="Add task"
        description={`Add a task to ${workflow.id}. It starts pending until you start the workflow.`}
        className="max-h-[85vh] overflow-y-auto"
      >
        <div className="space-y-3">
          <Field label="Task id" hint="lowercase concept id, e.g. new-api">
            <Input
              value={id}
              autoFocus
              onChange={(e) => setId(e.target.value)}
              placeholder="new-api"
              className="font-mono"
            />
          </Field>

          <Field label="Title">
            <Input
              value={title}
              onChange={(e) => setTitle(e.target.value)}
              placeholder="Build the new API surface"
            />
          </Field>

          <Field label="From template" hint="pick one to supply the spec, or type it below">
            <div className="flex flex-wrap gap-1.5">
              <Chip on={fromTemplate === null} onClick={() => setFromTemplate(null)}>
                none
              </Chip>
              {(templates ?? []).map((t) => (
                <Chip
                  key={t.id}
                  on={fromTemplate === t.id}
                  onClick={() => setFromTemplate(t.id)}
                >
                  {t.id}
                </Chip>
              ))}
            </div>
          </Field>

          {fromTemplate ? (
            <div className="rounded-md border border-accent/30 bg-accent-soft/20 px-3 py-2 text-[11px] text-muted-foreground">
              Spec supplied by template{" "}
              <span className="font-mono text-accent">{fromTemplate}</span>
              {pickedTemplate?.spec_template && (
                <p className="mt-1 line-clamp-3 whitespace-pre-wrap font-mono text-[10px] text-muted-foreground/80">
                  {pickedTemplate.spec_template}
                </p>
              )}
            </div>
          ) : (
            <Field label="Spec">
              <textarea
                value={spec}
                onChange={(e) => setSpec(e.target.value)}
                placeholder="The agent's brief: goal, constraints, done-criteria…"
                rows={4}
                className="flex w-full rounded-md border border-border bg-surface-2 px-3 py-2 text-xs leading-relaxed outline-none transition-colors placeholder:text-muted-foreground/70 focus-visible:border-accent/50 focus-visible:ring-2 focus-visible:ring-ring/40 font-mono"
              />
            </Field>
          )}

          <Field label="Depends on" hint="tasks in this workflow that must finish first">
            {existingTasks.length === 0 ? (
              <span className="block text-[11px] text-muted-foreground">
                No other tasks in this workflow yet.
              </span>
            ) : (
              <div className="flex flex-wrap gap-1.5">
                {existingTasks.map((d) => {
                  // The in-workflow reuse source is a dependency too; show it
                  // selected-and-locked so the implied edge is visible, not a
                  // surprise the backend adds silently.
                  const impliedByReuse = showReuse && reuseFrom === d && reuseIsInWorkflow;
                  return (
                    <Chip
                      key={d}
                      on={deps.includes(d) || impliedByReuse}
                      locked={impliedByReuse}
                      onClick={() => !impliedByReuse && toggleDep(d)}
                    >
                      {d}
                      {impliedByReuse && " (reuse)"}
                    </Chip>
                  );
                })}
              </div>
            )}
            {reuseImpliesDep && (
              <span className="block text-[10px] text-muted-foreground">
                <span className="font-mono text-accent">{reuseFrom}</span> is added
                as a dependency automatically because this task reuses its worktree.
              </span>
            )}
          </Field>

          <Field label="Owns" hint="comma-separated globs (optional)">
            <Input
              value={owns}
              onChange={(e) => setOwns(e.target.value)}
              placeholder="src/api/**"
              className="font-mono"
            />
          </Field>

          <AgentPicker
            tool={tool}
            model={model}
            effort={effort}
            onToolChange={setTool}
            onModelChange={setModel}
            onEffortChange={setEffort}
            labels={{ agent: "Agent", agentHint: "blank = inherit template/workflow default" }}
          />

          <Field
            label="Worktree mode override"
            hint={`effective: ${WORKTREE_MODES[effectiveMode].label} — ${WORKTREE_MODES[effectiveMode].hint}`}
          >
            <div className="flex gap-1 rounded-md border border-border bg-surface p-0.5">
              <button
                type="button"
                onClick={() => setOverride(null)}
                className={modeBtn(override === null)}
              >
                Inherit
              </button>
              {(["new", "shared", "reuse", "branch"] as WorktreeMode[]).map((m) => (
                <button
                  key={m}
                  type="button"
                  onClick={() => setOverride(m)}
                  className={modeBtn(override === m)}
                >
                  {WORKTREE_MODES[m].label}
                </button>
              ))}
            </div>
          </Field>

          {showReuse && (
            <Field
              label="Reuse from"
              hint="the task whose worktree to reuse — a task here becomes a dependency; a task in another workflow is waited on at claim time"
            >
              <div className="flex flex-wrap gap-1.5">
                <Chip on={reuseFrom === null} onClick={() => setReuseFrom(null)}>
                  none
                </Chip>
                {existingTasks.map((t) => (
                  <Chip
                    key={t}
                    on={reuseFrom === t}
                    onClick={() => setReuseFrom(t)}
                  >
                    {t}
                  </Chip>
                ))}
              </div>
              <Input
                value={reuseFrom ?? ""}
                onChange={(e) => setReuseFrom(e.target.value.trim() || null)}
                placeholder="or type a task id from another workflow"
                className="mt-1.5 font-mono"
              />
            </Field>
          )}
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
            disabled={!id.trim() || !title.trim() || add.isPending}
          >
            Add task
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}

function modeBtn(on: boolean): string {
  return cn(
    "flex-1 rounded px-2 py-1 text-[11px] font-medium transition-colors",
    on ? "bg-accent-soft/60 text-accent" : "text-muted-foreground hover:text-foreground",
  );
}

function Chip({
  on,
  onClick,
  locked = false,
  children,
}: {
  on: boolean;
  onClick: () => void;
  /** Selected and not toggleable — e.g. a dep implied by `reuse_from`. */
  locked?: boolean;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={locked}
      title={locked ? "Implied by reuse — can't be removed here" : undefined}
      className={cn(
        "rounded-full border px-2.5 py-0.5 font-mono text-[11px] font-medium transition-colors",
        on
          ? "border-accent/40 bg-accent-soft/50 text-accent"
          : "border-border bg-surface text-muted-foreground hover:text-foreground",
        locked && "cursor-default opacity-80",
      )}
    >
      {children}
    </button>
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
