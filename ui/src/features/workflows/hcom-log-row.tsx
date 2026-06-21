import { Badge } from "@/components/ui/badge";
import { cn } from "@/lib/utils/cn";
import { fullTime, shortTime } from "@/lib/utils/platform";
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
