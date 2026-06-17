import type { LucideIcon } from "lucide-react";
import { Card } from "@/components/ui/card";

/** A single headline metric tile. */
export function StatCard({
  label,
  value,
  icon: Icon,
  accent,
  hint,
}: {
  label: string;
  value: number | string;
  icon: LucideIcon;
  accent?: string;
  hint?: string;
}) {
  return (
    <Card className="p-4">
      <div className="flex items-center justify-between">
        <span className="text-xs font-medium text-muted-foreground">{label}</span>
        <Icon className="size-4" style={{ color: accent ?? "var(--color-muted-foreground)" }} />
      </div>
      <div className="mt-2 text-2xl font-semibold tabular-nums tracking-tight">{value}</div>
      {hint && <p className="mt-0.5 text-[11px] text-muted-foreground">{hint}</p>}
    </Card>
  );
}
