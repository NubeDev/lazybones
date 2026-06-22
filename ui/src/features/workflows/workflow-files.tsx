import { useMemo, useState } from "react";
import {
  ChevronDown,
  ChevronRight,
  File as FileIcon,
  FileText,
  Folder,
  FolderGit2,
  FolderOpen,
  GitCompare,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/empty-state";
import { Skeleton } from "@/components/ui/skeleton";
import { ApiError } from "@/lib/api/client";
import { useFileContent, useFileDiff, useFileTree } from "@/lib/hooks/use-files";
import type { FileStatus, TreeEntry } from "@/types/files";
import { DiffView } from "./diff-view";

function errMsg(e: unknown, fallback: string): string {
  return e instanceof ApiError ? e.message : fallback;
}

/** Repo file browser, VSCode-style: a single expandable tree on the left with
 *  changed files/folders colour-tagged for quick navigation, and a viewer on
 *  the right that toggles between file content and its diff. Operates on the
 *  workflow's repo (`dir`); `base` is the workflow's base branch so the tree
 *  and diff can also surface branch-vs-base changes. */
export function WorkflowFiles({
  dir,
  base,
}: {
  dir: string;
  base: string | null;
}) {
  const [file, setFile] = useState<string | null>(null);

  if (!dir.trim()) {
    return (
      <EmptyState
        icon={FolderGit2}
        title="No repository configured"
        description="This workflow's workspace has no repo path set, so there are no files to browse."
      />
    );
  }

  return (
    <div className="grid gap-4 lg:grid-cols-[minmax(0,20rem)_minmax(0,1fr)]">
      <FileTree dir={dir} base={base} selected={file} onOpen={setFile} />
      {file ? (
        <FileViewer dir={dir} rel={file} base={base} />
      ) : (
        <div className="hidden rounded-lg border border-dashed border-border-strong lg:grid lg:place-items-center">
          <p className="text-xs text-muted-foreground">
            Select a file to view it.
          </p>
        </div>
      )}
    </div>
  );
}

// ---- tree model ---------------------------------------------------------

interface Node {
  name: string;
  path: string;
  is_dir: boolean;
  status: FileStatus | null;
  children: Node[];
}

/** Build a nested tree from the flat, depth-first entry list, then roll a
 *  change status up to ancestor directories (a folder shows as changed if any
 *  descendant changed — "M" stands in for "has changes" on dirs). */
function buildTree(entries: TreeEntry[]): Node[] {
  const root: Node[] = [];
  const byPath = new Map<string, Node>();

  for (const e of entries) {
    const node: Node = { ...e, children: [] };
    byPath.set(e.path, node);
    const slash = e.path.lastIndexOf("/");
    if (slash === -1) {
      root.push(node);
    } else {
      const parent = byPath.get(e.path.slice(0, slash));
      // Parent always precedes its children (depth-first order), so it exists.
      (parent?.children ?? root).push(node);
    }
  }

  // Roll changed-state up to directories.
  const mark = (n: Node): boolean => {
    if (!n.is_dir) return n.status !== null;
    const changed = n.children.map(mark).some(Boolean);
    if (changed && n.status === null) n.status = "M";
    return changed;
  };
  root.forEach(mark);
  return root;
}

// ---- left: the tree -----------------------------------------------------

function FileTree({
  dir,
  base,
  selected,
  onOpen,
}: {
  dir: string;
  base: string | null;
  selected: string | null;
  onOpen: (rel: string) => void;
}) {
  const { data: entries, isLoading, error } = useFileTree(dir, base);
  const tree = useMemo(() => buildTree(entries ?? []), [entries]);

  return (
    <section className="rounded-lg border border-border bg-surface">
      <header className="border-b border-border px-3 py-2 text-xs font-medium text-muted-foreground">
        Files
      </header>
      <div className="max-h-[64vh] overflow-auto py-1">
        {isLoading && <Skeleton className="m-3 h-40" />}
        {error && (
          <p className="px-3 py-3 text-xs text-status-blocked">
            {errMsg(error, "Can't list files.")}
          </p>
        )}
        {entries && entries.length === 0 && !isLoading && (
          <p className="px-3 py-3 text-xs text-muted-foreground">
            No files tracked yet.
          </p>
        )}
        {tree.map((n) => (
          <TreeNode
            key={n.path}
            node={n}
            depth={0}
            selected={selected}
            onOpen={onOpen}
          />
        ))}
      </div>
    </section>
  );
}

/** Tailwind text colour for a git status tag (file or rolled-up dir). */
function statusColor(status: FileStatus | null): string {
  switch (status) {
    case "A":
    case "U":
      return "text-status-done";
    case "D":
      return "text-status-blocked";
    case "M":
      return "text-accent";
    default:
      return "";
  }
}

function TreeNode({
  node,
  depth,
  selected,
  onOpen,
}: {
  node: Node;
  depth: number;
  selected: string | null;
  onOpen: (rel: string) => void;
}) {
  // Top two levels start expanded — enough to orient without overwhelming.
  const [open, setOpen] = useState(depth < 1);
  const isSelected = !node.is_dir && node.path === selected;
  const color = statusColor(node.status);
  // 0.75rem indent per level, plus room for the chevron on files.
  const pad = 8 + depth * 12;

  return (
    <div>
      <button
        className={`group flex w-full items-center gap-1.5 py-1 pr-2 text-left text-sm hover:bg-surface-2/40 ${
          isSelected ? "bg-accent/10" : ""
        }`}
        style={{ paddingLeft: pad }}
        onClick={() => (node.is_dir ? setOpen((v) => !v) : onOpen(node.path))}
        title={node.path}
      >
        {node.is_dir ? (
          <>
            {open ? (
              <ChevronDown className="size-3.5 shrink-0 text-muted-foreground" />
            ) : (
              <ChevronRight className="size-3.5 shrink-0 text-muted-foreground" />
            )}
            {open ? (
              <FolderOpen className="size-4 shrink-0 text-muted-foreground" />
            ) : (
              <Folder className="size-4 shrink-0 text-muted-foreground" />
            )}
          </>
        ) : (
          <>
            <span className="size-3.5 shrink-0" />
            <FileIcon className="size-4 shrink-0 text-muted-foreground/70" />
          </>
        )}
        <span className={`truncate font-mono ${color || "text-foreground"}`}>
          {node.name}
        </span>
        {node.status && (
          <span className={`ml-auto shrink-0 text-[10px] font-semibold ${color}`}>
            {node.status}
          </span>
        )}
      </button>
      {node.is_dir && open && (
        <div>
          {node.children.map((c) => (
            <TreeNode
              key={c.path}
              node={c}
              depth={depth + 1}
              selected={selected}
              onOpen={onOpen}
            />
          ))}
        </div>
      )}
    </div>
  );
}

// ---- right: the viewer (content ↔ diff) ---------------------------------

type Mode = "content" | "diff";

function FileViewer({
  dir,
  rel,
  base,
}: {
  dir: string;
  rel: string;
  base: string | null;
}) {
  const [mode, setMode] = useState<Mode>("content");
  // When diffing, choose uncommitted (null) vs branch-vs-base (base).
  const [diffBase, setDiffBase] = useState<string | null>(null);

  return (
    <section className="min-w-0 rounded-lg border border-border bg-surface">
      <header className="flex flex-wrap items-center gap-2 border-b border-border px-3 py-2">
        <span className="truncate font-mono text-xs text-foreground">{rel}</span>
        <div className="ml-auto flex items-center gap-1">
          <ModeButton
            active={mode === "content"}
            onClick={() => setMode("content")}
            icon={FileText}
            label="Content"
          />
          <ModeButton
            active={mode === "diff"}
            onClick={() => setMode("diff")}
            icon={GitCompare}
            label="Diff"
          />
        </div>
      </header>

      {mode === "content" ? (
        <FileBody dir={dir} rel={rel} />
      ) : (
        <div className="p-3">
          {/* Which diff: uncommitted vs branch-vs-base (only if base known). */}
          <div className="mb-2 flex items-center gap-1">
            <DiffScopeButton
              active={diffBase === null}
              onClick={() => setDiffBase(null)}
              label="Uncommitted"
            />
            {base && (
              <DiffScopeButton
                active={diffBase === base}
                onClick={() => setDiffBase(base)}
                label={`vs ${base}`}
              />
            )}
          </div>
          <FileDiffBody dir={dir} rel={rel} base={diffBase} />
        </div>
      )}
    </section>
  );
}

function ModeButton({
  active,
  onClick,
  icon: Icon,
  label,
}: {
  active: boolean;
  onClick: () => void;
  icon: typeof FileText;
  label: string;
}) {
  return (
    <Button
      variant={active ? "secondary" : "ghost"}
      size="sm"
      onClick={onClick}
      className="gap-1"
    >
      <Icon className="size-3.5" /> {label}
    </Button>
  );
}

function DiffScopeButton({
  active,
  onClick,
  label,
}: {
  active: boolean;
  onClick: () => void;
  label: string;
}) {
  return (
    <button
      onClick={onClick}
      className={`rounded-full border px-2 py-0.5 text-[11px] ${
        active
          ? "border-accent bg-accent/10 text-accent"
          : "border-border text-muted-foreground hover:bg-surface-2/60"
      }`}
    >
      {label}
    </button>
  );
}

function FileBody({ dir, rel }: { dir: string; rel: string }) {
  const { data, isLoading, error } = useFileContent(dir, rel);
  if (isLoading) return <Skeleton className="m-3 h-64" />;
  if (error)
    return (
      <p className="px-3 py-3 text-xs text-status-blocked">
        {errMsg(error, "Can't read file.")}
      </p>
    );
  if (!data) return null;
  if (data.binary)
    return (
      <p className="px-3 py-6 text-center text-xs text-muted-foreground">
        Binary file — not shown.
      </p>
    );
  return (
    <pre className="max-h-[60vh] overflow-auto px-3 py-2 text-xs leading-relaxed">
      <code className="whitespace-pre">{data.content}</code>
    </pre>
  );
}

function FileDiffBody({
  dir,
  rel,
  base,
}: {
  dir: string;
  rel: string;
  base: string | null;
}) {
  const { data, isLoading, error } = useFileDiff(dir, base, rel);
  if (isLoading) return <Skeleton className="h-48" />;
  if (error)
    return (
      <p className="text-xs text-status-blocked">
        {errMsg(error, "Can't compute diff.")}
      </p>
    );
  if (!data || data.diff.trim() === "")
    return (
      <p className="py-6 text-center text-xs text-muted-foreground">
        No changes{base ? ` against ${base}` : ""}.
      </p>
    );
  return (
    <div className="max-h-[60vh]">
      <DiffView diff={data.diff} />
    </div>
  );
}
