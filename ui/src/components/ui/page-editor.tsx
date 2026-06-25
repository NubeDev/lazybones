import { useEffect, useLayoutEffect, useRef, useState } from "react";
import {
  Bold,
  Italic,
  Heading,
  List,
  ListOrdered,
  Code,
  Link as LinkIcon,
  Eye,
  FileCode,
} from "lucide-react";
import { useEditor, EditorContent, type Editor } from "@tiptap/react";
import StarterKit from "@tiptap/starter-kit";
import Placeholder from "@tiptap/extension-placeholder";
import { Markdown } from "tiptap-markdown";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils/cn";

/** The real printable A4 geometry the Typst PDF uses (see `lazybones-render`'s
 *  `render_pdf`): A4 210×297mm with 2.2cm side / 2.6cm top / 2.4cm bottom
 *  margins. The editor sheet matches it 1:1 so what the author types sits in the
 *  same box the PDF lays out — Typst stays the real pagination authority, this is
 *  the faithful on-screen sheet. */
const A4 = {
  widthMm: 210,
  heightMm: 297,
  marginXmm: 22,
  marginTopMm: 26,
  marginBottomMm: 24,
};
/** Printable content height in mm (what fits on one page before it overflows). */
const CONTENT_H_MM = A4.heightMm - A4.marginTopMm - A4.marginBottomMm;

/** A WYSIWYG page editor on a true A4 sheet. Edits rich text but round-trips to
 *  **markdown** (`value`/`onChange`) so storage and the markdown→Typst PDF
 *  pipeline are unchanged. The sheet is sized to the PDF's printable area and an
 *  overflow indicator warns when the content passes where the first page ends —
 *  an approximate cue (the browser measures differently than Typst), so the
 *  preview/PDF remain the source of truth.
 *
 *  Fully controlled: `value` is markdown in, `onChange` is markdown out. */
export function PageEditor({
  value,
  onChange,
  placeholder,
  className,
}: {
  value: string;
  onChange: (next: string) => void;
  placeholder?: string;
  className?: string;
}) {
  // Guard so programmatic `setContent` (re-seeding from a new saved revision)
  // doesn't echo back through `onChange` as a user edit.
  const applying = useRef(false);
  // "write" = WYSIWYG on the A4 sheet; "markdown" = raw markdown source. Both
  // bind to the same markdown `value`, so switching is lossless: edits in either
  // mode flow through `onChange`, and the WYSIWYG re-seeds from the (possibly
  // hand-edited) markdown when switching back.
  const [mode, setMode] = useState<"write" | "markdown">("write");

  const editor = useEditor({
    extensions: [
      StarterKit.configure({ heading: { levels: [1, 2, 3, 4, 5, 6] } }),
      Placeholder.configure({ placeholder: placeholder ?? "Write this page…" }),
      Markdown.configure({ html: false, transformPastedText: true, breaks: false }),
    ],
    content: value,
    editorProps: {
      attributes: {
        // The editable area IS the printable content box; padding/size come from
        // the sheet wrapper, so here we just make it fill and read as a page.
        class: "page-editor-prose focus:outline-none",
      },
    },
    onUpdate: ({ editor }) => {
      if (applying.current) return;
      onChange(getMarkdown(editor));
    },
  });

  // Re-seed the editor when the saved markdown changes from outside (e.g. an
  // asset inserted into the draft, or a fresh page loaded) without clobbering
  // in-progress typing: only replace when the incoming markdown differs from
  // what the editor currently holds.
  useEffect(() => {
    if (!editor) return;
    // Only re-seed when the incoming markdown differs *meaningfully* from what
    // the editor holds. Comparing trimmed forms avoids a re-seed (and the cursor
    // jump it causes) when the only difference is serializer-level whitespace
    // between the stored markdown and Tiptap's re-serialization of the same text.
    if (value.trim() !== getMarkdown(editor).trim()) {
      applying.current = true;
      editor.commands.setContent(value, { emitUpdate: false });
      applying.current = false;
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [value, editor]);

  const overflow = useOverflow(editor);

  if (!editor) return null;

  return (
    <div className={cn("space-y-2", className)}>
      {/* Header: the WYSIWYG toolbar (write mode only) and the Write/Markdown
          mode toggle. */}
      <div className="flex items-center justify-between gap-2">
        <div className={cn(mode !== "write" && "pointer-events-none opacity-40")}>
          <Toolbar editor={editor} />
        </div>
        <div className="flex items-center gap-0.5 rounded-md border border-border bg-surface-2 px-1 py-0.5">
          <Button
            type="button"
            variant={mode === "write" ? "secondary" : "ghost"}
            size="sm"
            className="h-6 px-2 text-[11px]"
            onClick={() => setMode("write")}
          >
            <Eye className="size-3" /> Write
          </Button>
          <Button
            type="button"
            variant={mode === "markdown" ? "secondary" : "ghost"}
            size="sm"
            className="h-6 px-2 text-[11px]"
            onClick={() => setMode("markdown")}
          >
            <FileCode className="size-3" /> Markdown
          </Button>
        </div>
      </div>

      {mode === "write" ? (
        <>
          {/* The A4 desk: a scaled, paper-coloured sheet centred on a grey
              surface, mirroring the preview's look so editing and previewing feel
              like one surface. */}
          <div className="flex justify-center rounded-md bg-[#e6e6e9] p-4 dark:bg-surface-2/40">
            <div className="page-sheet" data-overflow={overflow.over}>
              <EditorContent editor={editor} />
              {/* The first-page boundary line: where content starts spilling onto
                  a second page (approximate — Typst paginates the real PDF). */}
              {overflow.over && <div className="page-boundary" aria-hidden />}
            </div>
          </div>

          <div className="flex items-center justify-between text-[10px] text-muted-foreground">
            <span>A4 page · saved as markdown</span>
            {overflow.over ? (
              <span className="text-status-blocked">
                Content runs past one page — it will flow onto another page in the PDF.
              </span>
            ) : (
              <span>{overflow.pct}% of the page used</span>
            )}
          </div>
        </>
      ) : (
        /* Raw markdown source: edit the same `value` directly. Switching back to
           Write re-seeds the WYSIWYG from this text. */
        <>
          <textarea
            value={value}
            onChange={(e) => onChange(e.target.value)}
            placeholder={placeholder ?? "Write this page in markdown…"}
            rows={20}
            className="block w-full resize-y rounded-md border border-border bg-surface-2 px-3 py-2.5 font-mono text-xs leading-relaxed outline-none placeholder:text-muted-foreground/70 focus-visible:border-accent/50 focus-visible:ring-2 focus-visible:ring-ring/40"
          />
          <p className="text-[10px] text-muted-foreground">
            Markdown source · switch to Write for the A4 page view
          </p>
        </>
      )}
    </div>
  );
}

/** The current markdown of the editor via the `tiptap-markdown` serializer.
 *  The extension stores its serializer under `storage.markdown`, which it does
 *  not type-augment, so we read it through a narrow cast. */
function getMarkdown(editor: Editor): string {
  const storage = editor.storage as { markdown?: { getMarkdown(): string } };
  return storage.markdown?.getMarkdown() ?? "";
}

/** Measure how full the editable content is against one printable A4 page, in a
 *  layout effect so it tracks every content change. Returns whether it overran
 *  the first page and the fill percentage (clamped 0–100). */
function useOverflow(editor: Editor | null) {
  const [state, setState] = useState({ over: false, pct: 0 });

  useLayoutEffect(() => {
    if (!editor) return;
    const measure = () => {
      const el = editor.view.dom as HTMLElement;
      // The sheet sets the content box to CONTENT_H_MM via CSS; compare the
      // rendered scroll height against that box (read back in px from the live
      // element so the device's mm→px scale is exact).
      const boxPx = mmToPx(CONTENT_H_MM, el);
      const used = el.scrollHeight;
      const pct = Math.min(100, Math.round((used / boxPx) * 100));
      setState({ over: used > boxPx + 1, pct });
    };
    measure();
    editor.on("update", measure);
    const ro = new ResizeObserver(measure);
    ro.observe(editor.view.dom);
    return () => {
      editor.off("update", measure);
      ro.disconnect();
    };
  }, [editor]);

  return state;
}

/** Convert millimetres to device pixels using a probe element so the conversion
 *  honours the browser's real mm scale (independent of zoom/DPI assumptions). */
function mmToPx(mm: number, near: HTMLElement): number {
  const probe = document.createElement("div");
  probe.style.cssText = `position:absolute;visibility:hidden;height:${mm}mm;`;
  near.appendChild(probe);
  const px = probe.getBoundingClientRect().height;
  near.removeChild(probe);
  return px;
}

/** The formatting toolbar, wired to Tiptap commands with active-state styling. */
function Toolbar({ editor }: { editor: Editor }) {
  const tools = [
    {
      icon: Heading,
      title: "Heading",
      run: () => editor.chain().focus().toggleHeading({ level: 2 }).run(),
      active: editor.isActive("heading", { level: 2 }),
    },
    {
      icon: Bold,
      title: "Bold",
      run: () => editor.chain().focus().toggleBold().run(),
      active: editor.isActive("bold"),
    },
    {
      icon: Italic,
      title: "Italic",
      run: () => editor.chain().focus().toggleItalic().run(),
      active: editor.isActive("italic"),
    },
    {
      icon: Code,
      title: "Code",
      run: () => editor.chain().focus().toggleCode().run(),
      active: editor.isActive("code"),
    },
    {
      icon: List,
      title: "Bullet list",
      run: () => editor.chain().focus().toggleBulletList().run(),
      active: editor.isActive("bulletList"),
    },
    {
      icon: ListOrdered,
      title: "Numbered list",
      run: () => editor.chain().focus().toggleOrderedList().run(),
      active: editor.isActive("orderedList"),
    },
    {
      icon: LinkIcon,
      title: "Link",
      run: () => {
        const url = window.prompt("Link URL");
        if (url) editor.chain().focus().setLink({ href: url }).run();
      },
      active: editor.isActive("link"),
    },
  ];

  return (
    <div className="flex items-center gap-0.5 rounded-md border border-border bg-surface-2 px-1.5 py-1">
      {tools.map((t) => (
        <Button
          key={t.title}
          type="button"
          variant={t.active ? "secondary" : "ghost"}
          size="icon-sm"
          title={t.title}
          onClick={t.run}
        >
          <t.icon className="size-3.5" />
        </Button>
      ))}
    </div>
  );
}
