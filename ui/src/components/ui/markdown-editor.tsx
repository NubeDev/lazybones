import { useRef, useState } from "react";
import {
  Bold,
  Italic,
  Heading,
  List,
  ListOrdered,
  Code,
  Link as LinkIcon,
  Eye,
  Pencil,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Markdown } from "@/components/ui/markdown";
import { cn } from "@/lib/utils/cn";

/** A markdown editor: a monospace textarea with a formatting toolbar and a
 *  Write/Preview toggle. The toolbar wraps or prefixes the current selection so
 *  authoring a skill body feels like a real editor, not a bare textarea. Fully
 *  controlled via `value`/`onChange`. */
export function MarkdownEditor({
  value,
  onChange,
  placeholder,
  minRows = 16,
  className,
}: {
  value: string;
  onChange: (next: string) => void;
  placeholder?: string;
  minRows?: number;
  className?: string;
}) {
  const ref = useRef<HTMLTextAreaElement>(null);
  const [mode, setMode] = useState<"write" | "preview">("write");

  /** Wrap the current selection (or caret) in `before`/`after`. */
  function wrap(before: string, after = before) {
    const el = ref.current;
    if (!el) return;
    const { selectionStart: s, selectionEnd: e } = el;
    const sel = value.slice(s, e);
    const next = value.slice(0, s) + before + sel + after + value.slice(e);
    onChange(next);
    // Restore a sensible selection around the wrapped text after React re-renders.
    requestAnimationFrame(() => {
      el.focus();
      el.setSelectionRange(s + before.length, s + before.length + sel.length);
    });
  }

  /** Prefix each line of the current selection (or caret line) with `prefix`. */
  function prefixLines(prefix: string | ((i: number) => string)) {
    const el = ref.current;
    if (!el) return;
    const { selectionStart: s, selectionEnd: e } = el;
    const lineStart = value.lastIndexOf("\n", s - 1) + 1;
    const block = value.slice(lineStart, e);
    const prefixed = block
      .split("\n")
      .map((ln, i) => (typeof prefix === "string" ? prefix : prefix(i)) + ln)
      .join("\n");
    const next = value.slice(0, lineStart) + prefixed + value.slice(e);
    onChange(next);
    requestAnimationFrame(() => {
      el.focus();
      el.setSelectionRange(lineStart, lineStart + prefixed.length);
    });
  }

  const tools = [
    { icon: Heading, title: "Heading", run: () => prefixLines("## ") },
    { icon: Bold, title: "Bold", run: () => wrap("**") },
    { icon: Italic, title: "Italic", run: () => wrap("_") },
    { icon: Code, title: "Code", run: () => wrap("`") },
    { icon: List, title: "Bullet list", run: () => prefixLines("- ") },
    {
      icon: ListOrdered,
      title: "Numbered list",
      run: () => prefixLines((i) => `${i + 1}. `),
    },
    { icon: LinkIcon, title: "Link", run: () => wrap("[", "](url)") },
  ];

  return (
    <div className={cn("rounded-md border border-border bg-surface-2", className)}>
      <div className="flex items-center justify-between gap-2 border-b border-border px-1.5 py-1">
        <div className={cn("flex items-center gap-0.5", mode === "preview" && "opacity-40")}>
          {tools.map((t) => (
            <Button
              key={t.title}
              type="button"
              variant="ghost"
              size="icon-sm"
              title={t.title}
              disabled={mode === "preview"}
              onClick={t.run}
            >
              <t.icon className="size-3.5" />
            </Button>
          ))}
        </div>
        <div className="flex items-center gap-0.5">
          <Button
            type="button"
            variant={mode === "write" ? "secondary" : "ghost"}
            size="sm"
            className="h-6 px-2 text-[11px]"
            onClick={() => setMode("write")}
          >
            <Pencil className="size-3" /> Write
          </Button>
          <Button
            type="button"
            variant={mode === "preview" ? "secondary" : "ghost"}
            size="sm"
            className="h-6 px-2 text-[11px]"
            onClick={() => setMode("preview")}
          >
            <Eye className="size-3" /> Preview
          </Button>
        </div>
      </div>

      {mode === "write" ? (
        <textarea
          ref={ref}
          value={value}
          onChange={(e) => onChange(e.target.value)}
          placeholder={placeholder}
          rows={minRows}
          className="block w-full resize-y bg-transparent px-3 py-2.5 font-mono text-xs leading-relaxed outline-none placeholder:text-muted-foreground/70"
        />
      ) : (
        <div className="min-h-[8rem] px-3 py-2.5">
          {value.trim() ? (
            <Markdown>{value}</Markdown>
          ) : (
            <p className="text-xs text-muted-foreground">Nothing to preview yet.</p>
          )}
        </div>
      )}
    </div>
  );
}
