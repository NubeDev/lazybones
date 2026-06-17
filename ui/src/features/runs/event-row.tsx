import { ArrowRight } from "lucide-react";
import { StatusDot } from "@/components/ui/status-badge";
import { shortTime } from "@/lib/utils/platform";
import { STATUSES, type Status } from "@/types/task";
import type { RunEvent } from "@/types/event";

function asStatus(s: string): Status | null {
  return (STATUSES as string[]).includes(s) ? (s as Status) : null;
}

/** One transition in the run timeline. */
export function EventRow({ event }: { event: RunEvent }) {
  const from = asStatus(event.from);
  const to = asStatus(event.to);
  return (
    <li className="flex items-center gap-3 py-2.5 text-xs">
      <span className="w-32 shrink-0 font-mono text-[11px] text-muted-foreground">
        {shortTime(event.at)}
      </span>
      <span className="w-24 shrink-0 truncate font-mono font-semibold">{event.task}</span>
      <span className="flex items-center gap-1.5">
        {from ? <StatusDot status={from} /> : <span className="size-2" />}
        <span className="text-muted-foreground">{event.from}</span>
        <ArrowRight className="size-3 text-muted-foreground/60" />
        {to ? <StatusDot status={to} /> : <span className="size-2" />}
        <span className="font-medium">{event.to}</span>
      </span>
      <span className="ml-auto truncate text-[11px] text-muted-foreground">
        {event.actor}
      </span>
    </li>
  );
}
