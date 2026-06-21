import { cn } from "@/lib/utils/cn";
import type { WorktreeMode } from "@/types/task";

/** Operator-facing copy for each worktree provisioning mode. The loop reads the
 *  stored mode when it claims a task; this is how a human picks it. */
export const WORKTREE_MODES: Record<
  WorktreeMode,
  { label: string; hint: string }
> = {
  new: {
    label: "Isolated",
    hint: "Each task gets its own branch + worktree and runs in parallel. Best for independent tasks — but a workflow ends up with one branch (and one PR) per task.",
  },
  shared: {
    label: "Shared (one PR)",
    hint: "Every task in the workflow builds on ONE branch in ONE worktree, run one after another. The whole workflow becomes a single branch — so you review and merge just one PR.",
  },
  reuse: {
    label: "Reuse",
    hint: "Run in another task's existing worktree (via its reuse-from link) — continue exactly where it left off, no new checkout.",
  },
  branch: {
    label: "Main checkout",
    hint: "Run in the repo's main checkout on the task's branch; no separate worktree. Serial only.",
  },
};

const ORDER: WorktreeMode[] = ["new", "shared", "reuse", "branch"];

/** A small segmented control for choosing a task's worktree mode, with a
 *  plain-language explainer of the selected mode underneath (it changes how many
 *  branches/PRs a workflow produces, so the choice deserves a sentence). Shared
 *  by the authoring form (sets the default) and the Start popover (overrides at
 *  start). */
export function WorktreeModePicker({
  value,
  onChange,
}: {
  value: WorktreeMode;
  onChange: (mode: WorktreeMode) => void;
}) {
  return (
    <div className="space-y-1.5">
      <div className="flex gap-1 rounded-md border border-border bg-surface p-0.5">
        {ORDER.map((mode) => {
          const on = value === mode;
          return (
            <button
              key={mode}
              type="button"
              onClick={() => onChange(mode)}
              className={cn(
                "flex-1 rounded px-2 py-1 text-[11px] font-medium transition-colors",
                on
                  ? "bg-accent-soft/60 text-accent"
                  : "text-muted-foreground hover:text-foreground",
              )}
            >
              {WORKTREE_MODES[mode].label}
            </button>
          );
        })}
      </div>
      <p className="text-[11px] leading-snug text-muted-foreground">
        {WORKTREE_MODES[value].hint}
      </p>
    </div>
  );
}
