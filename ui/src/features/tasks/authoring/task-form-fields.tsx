import { Input } from "@/components/ui/input";
import { cn } from "@/lib/utils/cn";
import { AgentPicker } from "@/features/agents/agent-picker";
import { WORKTREE_MODES, WorktreeModePicker } from "../worktree-mode";
import type { TaskDraft } from "@/lib/api/tasks";

/** The shared editable fields for authoring a task (create + edit). The `id` is
 *  only editable on create; pass `lockId` for edit. `depCandidates` are other
 *  task ids offered as dependency toggle-chips (the caller excludes the current
 *  task so it can't depend on itself). */
export function TaskFormFields({
  id,
  onId,
  lockId,
  draft,
  onDraft,
  depCandidates,
}: {
  id: string;
  onId: (v: string) => void;
  lockId?: boolean;
  draft: TaskDraft;
  onDraft: (d: TaskDraft) => void;
  depCandidates: string[];
}) {
  function toggleDep(depId: string) {
    const on = draft.deps.includes(depId);
    onDraft({
      ...draft,
      deps: on ? draft.deps.filter((d) => d !== depId) : [...draft.deps, depId],
    });
  }

  return (
    <div className="space-y-3">
      <Field label="Task id" hint={lockId ? "id is fixed after creation" : "lowercase concept id, e.g. auth"}>
        <Input
          value={id}
          disabled={lockId}
          autoFocus={!lockId}
          onChange={(e) => onId(e.target.value)}
          placeholder="auth"
          className="font-mono"
        />
      </Field>

      <Field label="Title">
        <Input
          value={draft.title}
          autoFocus={lockId}
          onChange={(e) => onDraft({ ...draft, title: e.target.value })}
          placeholder="Scoped session + capability grants"
        />
      </Field>

      <Field label="Spec">
        <textarea
          value={draft.spec}
          onChange={(e) => onDraft({ ...draft, spec: e.target.value })}
          placeholder="The agent's brief: goal, constraints, done-criteria…"
          rows={5}
          className="flex w-full rounded-md border border-border bg-surface-2 px-3 py-2 text-xs leading-relaxed outline-none transition-colors placeholder:text-muted-foreground/70 focus-visible:border-accent/50 focus-visible:ring-2 focus-visible:ring-ring/40 font-mono"
        />
      </Field>

      <Field label="Depends on" hint="tasks that must be done first">
        {depCandidates.length === 0 ? (
          <span className="block text-[11px] text-muted-foreground">
            No other tasks to depend on yet.
          </span>
        ) : (
          <div className="flex flex-wrap gap-1.5">
            {depCandidates.map((cand) => {
              const on = draft.deps.includes(cand);
              return (
                <button
                  key={cand}
                  type="button"
                  onClick={() => toggleDep(cand)}
                  className={cn(
                    "rounded-full border px-2.5 py-0.5 font-mono text-[11px] font-medium transition-colors",
                    on
                      ? "border-accent/40 bg-accent-soft/50 text-accent"
                      : "border-border bg-surface text-muted-foreground hover:text-foreground",
                  )}
                >
                  {cand}
                </button>
              );
            })}
          </div>
        )}
      </Field>

      <Field label="Owns" hint="comma-separated globs">
        <Input
          value={draft.owns.join(", ")}
          onChange={(e) => onDraft({ ...draft, owns: splitList(e.target.value) })}
          placeholder="crates/auth/**"
          className="font-mono"
        />
      </Field>

      <AgentPicker
        tool={draft.tool ?? ""}
        model={draft.model ?? null}
        effort={draft.effort ?? null}
        onToolChange={(t) => onDraft({ ...draft, tool: t.trim() || null })}
        onModelChange={(m) => onDraft({ ...draft, model: m })}
        onEffortChange={(e) => onDraft({ ...draft, effort: e })}
        labels={{ agent: "Agent tool", agentHint: "blank = inherit run/workflow default" }}
      />

      <Field label="Worktree" hint={WORKTREE_MODES[draft.worktree_mode].hint}>
        <WorktreeModePicker
          value={draft.worktree_mode}
          onChange={(m) => onDraft({ ...draft, worktree_mode: m })}
        />
      </Field>
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

/** Parse a comma/space separated list into trimmed, non-empty entries. */
function splitList(raw: string): string[] {
  return raw
    .split(",")
    .map((s) => s.trim())
    .filter(Boolean);
}

/** A blank draft for the create dialog. */
export const EMPTY_DRAFT: TaskDraft = {
  title: "",
  spec: "",
  deps: [],
  owns: [],
  tool: null,
  model: null,
  effort: null,
  worktree_mode: "new",
};
