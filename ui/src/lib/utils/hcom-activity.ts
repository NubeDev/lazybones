/** Format raw hcom `status` events into readable, Claude-Code-style activity.
 *
 *  hcom emits a tool-status event as `{ context: "tool:Bash", detail: "<command
 *  or file path>", status: "active" }`. Rendered verbatim that's an unreadable
 *  JSON wall; this turns it into a clean one-line summary ("Running cargo build",
 *  "Editing health.rs") plus the full detail for an expandable view. */

export interface HcomActivity {
  /** Short human line: verb + the meaningful tail of the detail. */
  label: string;
  /** The full underlying detail (whole command / absolute path), or "" if none. */
  detail: string;
  /** The tool name parsed from `context` (e.g. "Bash"), or null. */
  tool: string | null;
}

/** The tool a `context` like `tool:Bash` names, or null for anything else. */
function toolOf(context: unknown): string | null {
  if (typeof context !== "string") return null;
  const m = context.match(/^tool:(.+)$/);
  return m ? m[1] : null;
}

/** A present, non-blank string field of `data`, else "". */
function strField(data: Record<string, unknown>, key: string): string {
  const v = data[key];
  return typeof v === "string" ? v.trim() : "";
}

/** Strip the absolute worktree/repo prefix so a path reads repo-relative
 *  ("crates/rubix-device/src/health.rs") the way Claude Code shows it. Anchors on
 *  the first recognizable repo root segment; falls back to the basename's parent
 *  chain, or the whole path if no anchor is found. */
export function repoRelative(path: string): string {
  if (typeof path !== "string" || path.length === 0) return path;
  // A worktree lives at <repo>/.lazy/wt/<task>/<repo-relative…>; cut everything
  // up to and including the task segment after `.lazy/wt/`.
  const wt = path.match(/\.lazy\/wt\/[^/]+\/(.+)$/);
  if (wt) return wt[1];
  // Otherwise anchor on a conventional source root if one is present.
  const root = path.match(/\/((?:crates|src|ui|tests|docs|bin)\/.+)$/);
  if (root) return root[1];
  // No anchor — keep it short with the last two segments.
  const parts = path.replace(/\/+$/, "").split("/");
  return parts.length <= 2 ? path : parts.slice(-2).join("/");
}

/** Collapse a shell command to its first line, trimmed, for the label. */
function firstLine(cmd: string): string {
  const line = cmd.split("\n", 1)[0].trim();
  return line.length > 80 ? `${line.slice(0, 79)}…` : line;
}

/** The meaningful command from a Bash detail: agents routinely prefix a `cd …`
 *  (joined by `&&`, `;`, or a newline) before the real work, which makes every
 *  Bash line read "Running cd …". Strip a leading `cd <path>` separator so the
 *  label shows what's actually being run (e.g. "cargo build -p rubix-ext"). */
function meaningfulCommand(cmd: string): string {
  const trimmed = cmd.trim();
  const m = trimmed.match(/^cd\s+\S+\s*(?:&&|;|\n)\s*([\s\S]+)$/);
  return firstLine(m ? m[1] : trimmed);
}

/** Parse a status-kind event's `data` into a readable activity, or null when the
 *  payload isn't a recognizable tool-status shape (caller falls back). */
export function parseHcomActivity(data: unknown): HcomActivity | null {
  if (!data || typeof data !== "object") return null;
  const obj = data as Record<string, unknown>;
  const tool = toolOf(obj.context);
  if (!tool) return null;

  const detail = strField(obj, "detail");

  switch (tool) {
    case "Bash": {
      // The command is the detail; show the meaningful command (past any `cd …`).
      const label = detail ? `Running ${meaningfulCommand(detail)}` : "Running a command";
      return { label, detail, tool };
    }
    case "Read": {
      const label = detail ? `Reading ${repoRelative(detail)}` : "Reading a file";
      return { label, detail, tool };
    }
    case "Edit":
    case "Write":
    case "NotebookEdit": {
      const label = detail ? `Editing ${repoRelative(detail)}` : "Editing a file";
      return { label, detail, tool };
    }
    case "Glob":
    case "Grep": {
      const label = detail ? `Searching ${detail}` : "Searching";
      return { label, detail, tool };
    }
    case "WebFetch":
    case "WebSearch": {
      const label = detail ? `Looking up ${detail}` : "Looking something up";
      return { label, detail, tool };
    }
    case "Task": {
      const label = detail ? `Delegating: ${firstLine(detail)}` : "Delegating a sub-task";
      return { label, detail, tool };
    }
    default: {
      const label = detail ? `${tool}: ${firstLine(detail)}` : `Using ${tool}`;
      return { label, detail, tool };
    }
  }
}
