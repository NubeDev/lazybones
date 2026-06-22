import type { CSSProperties } from "react";
import { cn } from "@/lib/utils/cn";
import { WORKFLOW_STATE_META } from "@/types/workflow-state-meta";
import type { WorkflowState } from "@/types/workflow";

/** A colored pill for a workflow's derived state — the workflow analogue of
 *  `StatusBadge`. The state is whatever the API returned; never recomputed. */
export function WorkflowStateBadge({
  state,
  className,
}: {
  state: WorkflowState;
  className?: string;
}) {
  const meta = WORKFLOW_STATE_META[state];
  const Icon = meta.icon;
  const spin = state === "running";

  return (
    <span
      className={cn(
        "inline-flex items-center gap-1.5 rounded-full border px-2 py-0.5 text-xs font-medium",
        className,
      )}
      title={meta.description}
      style={
        {
          color: meta.color,
          borderColor: `color-mix(in oklch, ${meta.color} 35%, transparent)`,
          backgroundColor: `color-mix(in oklch, ${meta.color} 12%, transparent)`,
        } as CSSProperties
      }
    >
      <Icon className={cn("size-3", spin && "animate-spin [animation-duration:2.5s]")} />
      {meta.label}
    </span>
  );
}
