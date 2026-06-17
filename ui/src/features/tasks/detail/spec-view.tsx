import { Skeleton } from "@/components/ui/skeleton";

/** Render the task spec. The spec is markdown-ish prose seeded from tasks/*.md;
 *  we show it as preformatted text to preserve structure without a heavy
 *  markdown dependency. */
export function SpecView({ spec, loading }: { spec: string; loading?: boolean }) {
  if (loading) {
    return (
      <div className="space-y-2">
        <Skeleton className="h-3 w-full" />
        <Skeleton className="h-3 w-5/6" />
        <Skeleton className="h-3 w-4/6" />
      </div>
    );
  }
  if (!spec.trim()) {
    return <p className="text-xs text-muted-foreground">No spec text.</p>;
  }
  return (
    <pre className="max-h-72 overflow-auto whitespace-pre-wrap rounded-md border border-border bg-surface-2 p-3 font-mono text-[11px] leading-relaxed text-foreground/90">
      {spec}
    </pre>
  );
}
