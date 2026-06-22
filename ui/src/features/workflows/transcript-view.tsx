import { FileText } from "lucide-react";
import { ScrollArea } from "@/components/ui/scroll-area";
import { fullTime, shortTime } from "@/lib/utils/platform";
import { repoRelative } from "@/lib/utils/hcom-activity";
import type { TranscriptEntry } from "@/types/event";

/** Normalize hcom's `transcript --json --full` payload (an array of step
 *  objects) into typed entries, tolerating an unexpected shape by returning
 *  null so the caller can fall back to a raw view. */
function asEntries(data: unknown): TranscriptEntry[] | null {
  if (!Array.isArray(data)) return null;
  return data.filter(
    (e): e is TranscriptEntry => !!e && typeof e === "object",
  );
}

/** Split an entry's `action` prose into individual narration lines — hcom packs
 *  a step's reasoning as newline-joined sentences, and Claude/Copilot show those
 *  one per line. Blank lines are dropped. */
function narrationLines(action: string | undefined): string[] {
  if (!action) return [];
  return action
    .split("\n")
    .map((l) => l.trim())
    .filter((l) => l.length > 0);
}

/** Render an agent's transcript as a Claude-Code-style narration stream — each
 *  reasoning line as its own step, with the files a step touched shown
 *  repo-relative. Falls back to a formatted JSON block if the payload isn't the
 *  expected array shape. */
export function TranscriptView({ data }: { data: unknown }) {
  const entries = asEntries(data);

  if (!entries) {
    return (
      <ScrollArea className="max-h-[45vh] rounded-md border border-border bg-surface-2">
        <pre className="whitespace-pre-wrap break-words p-3 font-mono text-[11px] text-foreground">
          {JSON.stringify(data, null, 2)}
        </pre>
      </ScrollArea>
    );
  }

  // Flatten to a flat list of narration lines, carrying each source step's files
  // + timestamp onto its last line so the file chips read as "did X, touched Y".
  const steps = entries.flatMap((e) => {
    const lines = narrationLines(e.action);
    if (lines.length === 0) return [];
    return lines.map((text, i) => ({
      text,
      // Attach files/timestamp to the final line of the step only.
      files: i === lines.length - 1 ? (e.files ?? []) : [],
      timestamp: i === lines.length - 1 ? e.timestamp : undefined,
    }));
  });

  if (steps.length === 0) {
    return (
      <p className="text-xs text-muted-foreground">
        No narration yet — the agent is just getting started.
      </p>
    );
  }

  return (
    <ScrollArea className="max-h-[50vh] rounded-md border border-border">
      <ol className="divide-y divide-border">
        {steps.map((s, i) => (
          <li key={i} className="flex gap-2.5 px-3 py-2">
            <span className="mt-1 size-1.5 shrink-0 rounded-full bg-accent/60" />
            <div className="min-w-0 flex-1 space-y-1">
              <p className="whitespace-pre-wrap break-words text-xs leading-relaxed text-foreground">
                {s.text}
              </p>
              {(s.files.length > 0 || s.timestamp) && (
                <div className="flex flex-wrap items-center gap-1.5">
                  {s.timestamp && (
                    <span
                      className="font-mono text-[10px] text-muted-foreground"
                      title={fullTime(s.timestamp)}
                    >
                      {shortTime(s.timestamp)}
                    </span>
                  )}
                  {s.files.map((f, fi) => (
                    <span
                      key={`${f}-${fi}`}
                      title={f}
                      className="inline-flex items-center gap-1 rounded bg-surface-2 px-1.5 py-0.5 font-mono text-[10px] text-muted-foreground"
                    >
                      <FileText className="size-2.5" />
                      {repoRelative(f)}
                    </span>
                  ))}
                </div>
              )}
            </div>
          </li>
        ))}
      </ol>
    </ScrollArea>
  );
}
