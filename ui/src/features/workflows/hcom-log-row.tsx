import { useState } from "react";
import { ChevronRight } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { cn } from "@/lib/utils/cn";
import { fullTime, shortTime } from "@/lib/utils/platform";
import { parseHcomActivity } from "@/lib/utils/hcom-activity";
import type { HcomLogEntry, HcomLogKind } from "@/types/event";

const KIND_VARIANT: Record<HcomLogKind, "default" | "accent" | "outline"> = {
  message: "accent",
  status: "outline",
  life: "default",
};

/** Best-effort human text for an entry. A `message` carries `{ text, … }`; other
 *  kinds fall back to a compact JSON rendering of their payload. */
export function entryText(e: HcomLogEntry): string {
  const d = e.data;
  if (d && typeof d === "object" && "text" in d) {
    const t = (d as { text?: unknown }).text;
    if (typeof t === "string") return t;
  }
  if (typeof d === "string") return d;
  try {
    return JSON.stringify(d);
  } catch {
    return "";
  }
}

function isTruncated(e: HcomLogEntry): boolean {
  return (
    !!e.data &&
    typeof e.data === "object" &&
    (e.data as { truncated?: boolean }).truncated === true
  );
}

/** One hcom log entry: timestamp, kind badge, optional task label, and the
 *  payload text. `onLoadTranscript` (when given) surfaces the deep-view hint on
 *  truncated messages. */
export function HcomLogRow({
  entry,
  showTask = true,
  onLoadTranscript,
}: {
  entry: HcomLogEntry;
  showTask?: boolean;
  onLoadTranscript?: () => void;
}) {
  // A `status` event is a tool-activity tick — render it as a readable line
  // ("Editing health.rs") with the full command/path tucked behind an expander,
  // instead of dumping the raw `{context,detail,status}` JSON.
  const activity =
    entry.kind === "status" ? parseHcomActivity(entry.data) : null;
  if (activity) {
    return (
      <ActivityRow entry={entry} label={activity.label} detail={activity.detail} showTask={showTask} />
    );
  }

  return (
    <li className="flex gap-3 py-2 text-xs">
      <span
        className="w-20 shrink-0 font-mono text-[11px] text-muted-foreground"
        title={fullTime(entry.at)}
      >
        {shortTime(entry.at)}
      </span>
      <Badge variant={KIND_VARIANT[entry.kind]} className="h-5 shrink-0 px-2">
        {entry.kind}
      </Badge>
      <div className="min-w-0 flex-1">
        {showTask && (
          <span className="mr-2 font-mono text-[11px] font-semibold text-muted-foreground">
            {entry.task ?? "—"}
          </span>
        )}
        <span className="whitespace-pre-wrap break-words text-foreground">
          {entryText(entry)}
        </span>
        {isTruncated(entry) && (
          <button
            type="button"
            onClick={onLoadTranscript}
            className={cn(
              "ml-2 text-[11px] text-accent",
              onLoadTranscript ? "hover:underline" : "cursor-default opacity-70",
            )}
          >
            (truncated{onLoadTranscript ? " — load full transcript" : ""})
          </button>
        )}
      </div>
      <span className="ml-auto shrink-0 truncate text-[11px] text-muted-foreground">
        {entry.agent}
      </span>
    </li>
  );
}

/** A tool-activity tick: a clean "verb + target" line, click to reveal the full
 *  underlying command or file path (only when there is one). */
function ActivityRow({
  entry,
  label,
  detail,
  showTask,
}: {
  entry: HcomLogEntry;
  label: string;
  detail: string;
  showTask: boolean;
}) {
  const [open, setOpen] = useState(false);
  const expandable = detail.length > 0;

  return (
    <li className="flex gap-3 py-2 text-xs">
      <span
        className="w-20 shrink-0 font-mono text-[11px] text-muted-foreground"
        title={fullTime(entry.at)}
      >
        {shortTime(entry.at)}
      </span>
      <div className="min-w-0 flex-1">
        <button
          type="button"
          onClick={() => expandable && setOpen((o) => !o)}
          className={cn(
            "flex w-full items-start gap-1.5 text-left",
            expandable ? "cursor-pointer" : "cursor-default",
          )}
          aria-expanded={expandable ? open : undefined}
        >
          <ChevronRight
            className={cn(
              "mt-0.5 size-3 shrink-0 text-muted-foreground transition-transform",
              expandable ? "opacity-100" : "opacity-0",
              open && "rotate-90",
            )}
          />
          {showTask && (
            <span className="font-mono text-[11px] font-semibold text-muted-foreground">
              {entry.task ?? "—"}
            </span>
          )}
          <span className="break-words text-muted-foreground">{label}</span>
        </button>
        {open && expandable && (
          <pre className="mt-1 ml-[18px] whitespace-pre-wrap break-words rounded bg-surface-2 p-2 font-mono text-[11px] text-foreground">
            {detail}
          </pre>
        )}
      </div>
      <span className="ml-auto shrink-0 truncate text-[11px] text-muted-foreground">
        {entry.agent}
      </span>
    </li>
  );
}
