import { useMemo } from "react";

/** Colour class for one unified-diff line, keyed off its leading char. */
function lineClass(line: string): string {
  if (line.startsWith("+++") || line.startsWith("---"))
    return "text-muted-foreground";
  if (line.startsWith("@@")) return "text-accent";
  if (line.startsWith("diff ") || line.startsWith("index "))
    return "text-muted-foreground/70";
  if (line.startsWith("+")) return "bg-status-done/10 text-status-done";
  if (line.startsWith("-")) return "bg-status-blocked/10 text-status-blocked";
  return "text-foreground/80";
}

/** Render a unified `git diff` with per-line colouring. Empty diff renders an
 *  "no changes" note via the parent (we just render the lines here). */
export function DiffView({ diff }: { diff: string }) {
  const lines = useMemo(() => diff.replace(/\n$/, "").split("\n"), [diff]);
  return (
    <pre className="overflow-auto rounded-md border border-border bg-surface-2/30 text-xs leading-relaxed">
      <code className="block">
        {lines.map((line, i) => (
          <span
            key={i}
            className={`block whitespace-pre px-3 ${lineClass(line)}`}
          >
            {line || " "}
          </span>
        ))}
      </code>
    </pre>
  );
}
