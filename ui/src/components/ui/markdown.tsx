import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { cn } from "@/lib/utils/cn";

/** Render markdown to styled HTML. GitHub-flavoured (tables, task lists, strike-
 *  through). The project has no `@tailwindcss/typography`, so each element is
 *  styled inline to match the app's tokens — kept deliberately compact for the
 *  dense dashboard. Untrusted HTML is never rendered: react-markdown escapes raw
 *  HTML by default (no `rehype-raw`), so this is XSS-safe for operator content. */
export function Markdown({
  children,
  className,
}: {
  children: string;
  className?: string;
}) {
  return (
    <div className={cn("text-sm leading-relaxed text-foreground/90", className)}>
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        components={{
          h1: ({ node, ...p }) => (
            <h1 className="mt-4 mb-2 text-base font-semibold tracking-tight first:mt-0" {...p} />
          ),
          h2: ({ node, ...p }) => (
            <h2 className="mt-4 mb-2 text-sm font-semibold tracking-tight first:mt-0" {...p} />
          ),
          h3: ({ node, ...p }) => (
            <h3 className="mt-3 mb-1.5 text-xs font-semibold uppercase tracking-wide text-muted-foreground first:mt-0" {...p} />
          ),
          p: ({ node, ...p }) => <p className="my-2 first:mt-0 last:mb-0" {...p} />,
          ul: ({ node, ...p }) => (
            <ul className="my-2 ml-5 list-disc space-y-1 marker:text-muted-foreground" {...p} />
          ),
          ol: ({ node, ...p }) => (
            <ol className="my-2 ml-5 list-decimal space-y-1 marker:text-muted-foreground" {...p} />
          ),
          li: ({ node, ...p }) => <li className="pl-1" {...p} />,
          a: ({ node, ...p }) => (
            <a
              className="text-accent underline underline-offset-2 hover:text-accent/80"
              target="_blank"
              rel="noreferrer"
              {...p}
            />
          ),
          blockquote: ({ node, ...p }) => (
            <blockquote
              className="my-2 border-l-2 border-border pl-3 text-muted-foreground italic"
              {...p}
            />
          ),
          hr: () => <hr className="my-4 border-border" />,
          code: ({ node, className: c, children, ...p }) => {
            const inline = !String(c ?? "").includes("language-");
            return inline ? (
              <code
                className="rounded bg-surface-2 px-1 py-0.5 font-mono text-[0.85em] text-foreground/90"
                {...p}
              >
                {children}
              </code>
            ) : (
              <code className={cn("font-mono text-[12px]", c)} {...p}>
                {children}
              </code>
            );
          },
          pre: ({ node, ...p }) => (
            <pre
              className="my-2 overflow-auto rounded-md border border-border bg-surface-2 p-3 font-mono text-[12px] leading-relaxed"
              {...p}
            />
          ),
          table: ({ node, ...p }) => (
            <div className="my-2 overflow-x-auto">
              <table className="w-full border-collapse text-xs" {...p} />
            </div>
          ),
          th: ({ node, ...p }) => (
            <th
              className="border border-border bg-surface px-2 py-1 text-left font-semibold"
              {...p}
            />
          ),
          td: ({ node, ...p }) => (
            <td className="border border-border px-2 py-1 align-top" {...p} />
          ),
        }}
      >
        {children}
      </ReactMarkdown>
    </div>
  );
}
