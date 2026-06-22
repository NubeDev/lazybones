import { useState } from "react";
import {
  ExternalLink,
  FileText,
  Link as LinkIcon,
  Paperclip,
  Trash2,
  Upload,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { EmptyState } from "@/components/ui/empty-state";
import { Skeleton } from "@/components/ui/skeleton";
import { ApiError } from "@/lib/api/client";
import { assetUrl } from "@/lib/api/assets";
import {
  useAddLinkSource,
  useRemoveSource,
  useSources,
  useUploadFileSource,
} from "@/lib/hooks/use-documents";
import type { Source } from "@/types/document";

/** The uploads / sources panel: research material an author adds *behind* a
 *  document (links, uploaded PDFs/images). These never render into the output —
 *  that is what distinguishes them from references. Add links, upload files,
 *  preview a PDF's extracted text, and delete. */
export function DocumentSources({ documentId }: { documentId: string }) {
  const { data: sources, isLoading, error } = useSources(documentId);
  const addLink = useAddLinkSource();
  const upload = useUploadFileSource();
  const remove = useRemoveSource();

  const [url, setUrl] = useState("");
  const [title, setTitle] = useState("");

  function submitLink() {
    const u = url.trim();
    if (!u) return;
    addLink.mutate(
      { id: documentId, url: u, title: title.trim() },
      {
        onSuccess: () => {
          setUrl("");
          setTitle("");
        },
      },
    );
  }

  function onFiles(files: FileList | null) {
    if (!files) return;
    for (const file of Array.from(files)) {
      upload.mutate({ id: documentId, file });
    }
  }

  return (
    <div className="space-y-4">
      {/* Add a link. */}
      <div className="space-y-2 rounded-md border border-border bg-surface-2/40 p-3">
        <p className="flex items-center gap-1.5 text-xs font-medium">
          <LinkIcon className="size-3.5" /> Add a link
        </p>
        <Input
          value={url}
          onChange={(e) => setUrl(e.target.value)}
          placeholder="https://example.com/reference"
          className="font-mono text-xs"
        />
        <div className="flex gap-2">
          <Input
            value={title}
            onChange={(e) => setTitle(e.target.value)}
            placeholder="Title (optional)"
          />
          <Button size="sm" onClick={submitLink} disabled={!url.trim() || addLink.isPending}>
            Add
          </Button>
        </div>
        {addLink.error && (
          <p className="text-[11px] text-status-blocked">
            {addLink.error instanceof ApiError ? addLink.error.message : "Could not add the link."}
          </p>
        )}
      </div>

      {/* Upload files. */}
      <div className="flex items-center justify-between gap-2">
        <p className="text-xs text-muted-foreground">
          {sources ? `${sources.length} source${sources.length === 1 ? "" : "s"}` : "Loading…"}
        </p>
        <label>
          <input
            type="file"
            accept="application/pdf,image/*"
            multiple
            className="hidden"
            onChange={(e) => onFiles(e.target.files)}
          />
          <Button asChild size="sm" variant="secondary" disabled={upload.isPending}>
            <span>
              <Upload /> {upload.isPending ? "Uploading…" : "Upload PDF / image"}
            </span>
          </Button>
        </label>
      </div>

      {upload.error && (
        <p className="rounded-md border border-status-blocked/30 bg-status-blocked/10 px-3 py-2 text-xs text-status-blocked">
          {upload.error instanceof ApiError ? upload.error.message : "Upload failed."}
        </p>
      )}

      {error ? (
        <EmptyState
          icon={Paperclip}
          title="Can't load sources"
          description={error instanceof ApiError ? error.message : "Unexpected error"}
        />
      ) : isLoading && !sources ? (
        <Skeleton className="h-24 w-full" />
      ) : !sources || sources.length === 0 ? (
        <EmptyState
          icon={Paperclip}
          title="No sources yet"
          description="Add links or upload PDFs/images as context behind this document. They are never rendered into the output."
        />
      ) : (
        <ul className="divide-y divide-border rounded-md border border-border">
          {sources.map((s) => (
            <SourceRow
              key={s.id}
              source={s}
              onDelete={() => remove.mutate({ id: documentId, sid: s.id })}
              deleting={remove.isPending}
            />
          ))}
        </ul>
      )}
    </div>
  );
}

function SourceRow({
  source,
  onDelete,
  deleting,
}: {
  source: Source;
  onDelete: () => void;
  deleting: boolean;
}) {
  const [showText, setShowText] = useState(false);
  const isLink = source.kind === "link";
  const isPdf = source.content_type === "application/pdf";
  const href = isLink ? source.url ?? undefined : source.asset_id ? assetUrl(source.asset_id) : undefined;

  return (
    <li className="px-3 py-2.5">
      <div className="flex items-start gap-3">
        {isLink ? (
          <LinkIcon className="mt-0.5 size-4 shrink-0 text-muted-foreground" />
        ) : (
          <FileText className="mt-0.5 size-4 shrink-0 text-accent" />
        )}
        <div className="min-w-0 flex-1">
          <p className="truncate text-sm font-medium">{source.title || source.url || source.id}</p>
          <p className="truncate text-[11px] text-muted-foreground">
            {isLink ? source.url : source.content_type || "file"}
          </p>
          {source.extracted_text && (
            <button
              className="mt-1 text-[11px] text-accent hover:underline"
              onClick={() => setShowText((v) => !v)}
            >
              {showText ? "Hide" : "Preview"} extracted text
            </button>
          )}
        </div>
        <div className="flex shrink-0 items-center gap-1">
          {href && (
            <a
              href={href}
              target="_blank"
              rel="noreferrer"
              className="rounded p-1 text-muted-foreground hover:text-foreground"
              title="Open"
            >
              <ExternalLink className="size-3.5" />
            </a>
          )}
          <Button
            variant="ghost"
            size="sm"
            className="text-status-blocked"
            onClick={onDelete}
            disabled={deleting}
            title="Delete source"
          >
            <Trash2 className="size-3.5" />
          </Button>
        </div>
      </div>

      {showText && source.extracted_text && (
        <pre className="mt-2 max-h-48 overflow-auto whitespace-pre-wrap rounded-md border border-border bg-surface-2 p-2.5 text-[11px] leading-relaxed text-foreground/80">
          {source.extracted_text}
        </pre>
      )}
      {!isLink && isPdf && !source.extracted_text && (
        <p className="mt-1 text-[10px] text-muted-foreground">No extractable text in this PDF.</p>
      )}
    </li>
  );
}
