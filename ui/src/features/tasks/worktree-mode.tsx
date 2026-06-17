import { cn } from "@/lib/utils/cn";
import type { WorktreeMode } from "@/types/task";

/** Operator-facing copy for each worktree provisioning mode. The loop reads the
 *  stored mode when it claims a task; this is how a human picks it. */
export const WORKTREE_MODES: Record<
  WorktreeMode,
  { label: string; hint: string }
> = {
  new: {
    label: "New worktree",
    hint: "Isolated git worktree on a fresh branch (default).",
  },
  reuse: {
    label: "Reuse worktree",
    hint: "Run in the task's existing worktree path — no new checkout.",
  },
  branch: {
    label: "Same branch",
    hint: "Run in the main checkout on the task's branch; no worktree.",
  },
};

const ORDER: WorktreeMode[] = ["new", "reuse", "branch"];

/** A small segmented control for choosing a task's worktree mode. Shared by the
 *  authoring form (sets the default) and the Start popover (overrides at start). */
export function WorktreeModePicker({
  value,
  onChange,
}: {
  value: WorktreeMode;
  onChange: (mode: WorktreeMode) => void;
}) {
  return (
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
  );
}
