import type { ReactNode } from "react";

/** A labeled key/value row in the task detail panel. */
export function FieldRow({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex items-start justify-between gap-4 py-2 text-xs">
      <span className="shrink-0 text-muted-foreground">{label}</span>
      <span className="min-w-0 text-right font-medium text-foreground">{children}</span>
    </div>
  );
}

/** A monospace, copy-friendly value (truncates long shas/paths). */
export function Mono({ children }: { children: ReactNode }) {
  return (
    <span className="block truncate font-mono text-[11px] text-foreground">{children}</span>
  );
}
