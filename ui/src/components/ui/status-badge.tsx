import type { CSSProperties } from "react";
import { cn } from "@/lib/utils/cn";
import { STATUS_META } from "@/types/status-meta";
import type { Status } from "@/types/task";

interface StatusBadgeProps {
  status: Status;
  className?: string;
  /** Hide the text label, showing only the icon. */
  iconOnly?: boolean;
}

/** A pill carrying a status's color, icon, and label — the canonical status
 *  affordance reused across the board, table, and detail header. */
export function StatusBadge({ status, className, iconOnly }: StatusBadgeProps) {
  const meta = STATUS_META[status];
  const Icon = meta.icon;
  const spin = status === "running" || status === "gating";

  return (
    <span
      className={cn(
        "inline-flex items-center gap-1.5 rounded-full border px-2 py-0.5 text-xs font-medium",
        className,
      )}
      style={
        {
          color: meta.color,
          borderColor: `color-mix(in oklch, ${meta.color} 35%, transparent)`,
          backgroundColor: `color-mix(in oklch, ${meta.color} 12%, transparent)`,
        } as CSSProperties
      }
    >
      <Icon className={cn("size-3", spin && "animate-spin [animation-duration:2.5s]")} />
      {!iconOnly && meta.label}
    </span>
  );
}

/** A bare colored dot for dense lists. */
export function StatusDot({ status, className }: { status: Status; className?: string }) {
  const meta = STATUS_META[status];
  const live = status === "running";
  return (
    <span
      className={cn("inline-block size-2 rounded-full", live && "animate-pulse-ring", className)}
      style={{ backgroundColor: meta.color, color: meta.color }}
    />
  );
}
