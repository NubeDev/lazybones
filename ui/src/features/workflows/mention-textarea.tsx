import { useMemo, useRef, useState } from "react";
import type { KeyboardEvent, ChangeEvent } from "react";
import { cn } from "@/lib/utils/cn";

/** The `@token` being typed: a `@` that starts the string or follows whitespace,
 *  followed by GitHub-login characters, anchored to the caret. */
const MENTION_RE = /(^|\s)@([A-Za-z0-9-]*)$/;

/** A textarea with GitHub-style `@`-mention autocomplete. As the user types
 *  `@…`, it suggests matching `users` (repo collaborators) and inserts the
 *  picked login. Mentions are plain text in the body — GitHub turns them into
 *  real notifications when the comment is posted. */
export function MentionTextarea({
  value,
  onChange,
  users,
  placeholder,
  rows = 2,
  className,
}: {
  value: string;
  onChange: (next: string) => void;
  users: string[];
  placeholder?: string;
  rows?: number;
  className?: string;
}) {
  const ref = useRef<HTMLTextAreaElement>(null);
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  /** Index of the active token's `@` in `value`, so we can splice on accept. */
  const [tokenAt, setTokenAt] = useState(0);
  const [highlight, setHighlight] = useState(0);

  const matches = useMemo(() => {
    if (!open) return [];
    const q = query.toLowerCase();
    return users.filter((u) => u.toLowerCase().includes(q)).slice(0, 6);
  }, [open, query, users]);

  /** Re-evaluate whether the caret sits inside an `@mention` token. */
  function syncMention(el: HTMLTextAreaElement) {
    const caret = el.selectionStart ?? el.value.length;
    const m = el.value.slice(0, caret).match(MENTION_RE);
    if (m && users.length > 0) {
      setOpen(true);
      setQuery(m[2]);
      setTokenAt(caret - m[2].length - 1);
      setHighlight(0);
    } else {
      setOpen(false);
    }
  }

  function handleChange(e: ChangeEvent<HTMLTextAreaElement>) {
    onChange(e.target.value);
    syncMention(e.target);
  }

  function accept(login: string) {
    const el = ref.current;
    if (!el) return;
    const caret = el.selectionStart ?? value.length;
    const next = `${value.slice(0, tokenAt)}@${login} ${value.slice(caret)}`;
    onChange(next);
    setOpen(false);
    // Restore the caret just after the inserted "@login ".
    const pos = tokenAt + login.length + 2;
    requestAnimationFrame(() => {
      el.focus();
      el.setSelectionRange(pos, pos);
    });
  }

  function handleKeyDown(e: KeyboardEvent<HTMLTextAreaElement>) {
    if (!open || matches.length === 0) return;
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setHighlight((h) => (h + 1) % matches.length);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setHighlight((h) => (h - 1 + matches.length) % matches.length);
    } else if (e.key === "Enter" || e.key === "Tab") {
      e.preventDefault();
      accept(matches[highlight]);
    } else if (e.key === "Escape") {
      e.preventDefault();
      setOpen(false);
    }
  }

  return (
    <div className="relative">
      <textarea
        ref={ref}
        value={value}
        onChange={handleChange}
        onKeyDown={handleKeyDown}
        onClick={(e) => syncMention(e.currentTarget)}
        // Defer close so a mousedown-accept on the list still registers.
        onBlur={() => setTimeout(() => setOpen(false), 120)}
        placeholder={placeholder}
        rows={rows}
        className={cn(
          "w-full resize-y rounded-md border border-border bg-surface px-2 py-1.5 text-sm outline-none focus-visible:ring-2 focus-visible:ring-ring/70",
          className,
        )}
      />
      {open && matches.length > 0 && (
        <ul className="absolute left-2 top-full z-50 mt-1 w-52 overflow-hidden rounded-md border border-border bg-surface p-1 shadow-2xl">
          {matches.map((u, i) => (
            <li key={u}>
              <button
                type="button"
                // mousedown (not click) fires before the textarea blur; preventDefault
                // keeps focus so the caret restore lands correctly.
                onMouseDown={(e) => {
                  e.preventDefault();
                  accept(u);
                }}
                className={cn(
                  "flex w-full cursor-pointer select-none items-center rounded-md px-2.5 py-1.5 text-left text-xs",
                  i === highlight ? "bg-muted text-foreground" : "text-muted-foreground",
                )}
              >
                @{u}
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
