import { useState } from "react";
import { Copy, FileImage, Trash2, Upload } from "lucide-react";
import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/empty-state";
import { Skeleton } from "@/components/ui/skeleton";
import { ApiError } from "@/lib/api/client";
import { assetUrl } from "@/lib/api/assets";
import { useAssets, useDeleteAsset, useUploadAsset } from "@/lib/hooks/use-assets";
import type { Asset } from "@/types/asset";

/** The asset library: upload/list/delete content-addressed files (logos,
 *  diagrams, images). Bytes are deduped by sha256 server-side, so re-uploading
 *  an identical file is harmless. Reusable across documents; an asset's id is
 *  what a brand logo or an inline `![alt](<asset-id>)` image points at.
 *
 *  When embedded in the editor, `onInsert` adds an "Insert" action so a picked
 *  asset drops into the markdown body. */
export function AssetsLibrary({ onInsert }: { onInsert?: (asset: Asset) => void }) {
  const { data: assets, isLoading, error } = useAssets();
  const upload = useUploadAsset();
  const del = useDeleteAsset();

  function onFiles(files: FileList | null) {
    if (!files) return;
    for (const file of Array.from(files)) {
      upload.mutate({ file });
    }
  }

  const uploadButton = (
    <label>
      <input
        type="file"
        accept="image/*"
        multiple
        className="hidden"
        onChange={(e) => onFiles(e.target.files)}
      />
      <Button asChild size="sm" disabled={upload.isPending}>
        <span>
          <Upload /> {upload.isPending ? "Uploading…" : "Upload"}
        </span>
      </Button>
    </label>
  );

  return (
    <div className="space-y-3">
      <div className="flex items-center justify-between gap-2">
        <p className="text-xs text-muted-foreground">
          {assets ? `${assets.length} asset${assets.length === 1 ? "" : "s"}` : "Loading…"}
        </p>
        {uploadButton}
      </div>

      {upload.error && (
        <p className="rounded-md border border-status-blocked/30 bg-status-blocked/10 px-3 py-2 text-xs text-status-blocked">
          {upload.error instanceof ApiError ? upload.error.message : "Upload failed."}
        </p>
      )}

      {error ? (
        <EmptyState
          icon={FileImage}
          title="Can't load assets"
          description={error instanceof ApiError ? error.message : "Unexpected error"}
        />
      ) : isLoading && !assets ? (
        <div className="grid grid-cols-2 gap-3 sm:grid-cols-3 lg:grid-cols-4">
          {Array.from({ length: 4 }).map((_, i) => (
            <Skeleton key={i} className="h-32 w-full" />
          ))}
        </div>
      ) : !assets || assets.length === 0 ? (
        <EmptyState
          icon={FileImage}
          title="No assets yet"
          description="Upload logos and images once; reuse them across documents and brands."
          action={uploadButton}
        />
      ) : (
        <div className="grid grid-cols-2 gap-3 sm:grid-cols-3 lg:grid-cols-4">
          {assets.map((a) => (
            <AssetCard
              key={a.id}
              asset={a}
              onInsert={onInsert}
              onDelete={() => del.mutate(a.id)}
              deleting={del.isPending}
            />
          ))}
        </div>
      )}
    </div>
  );
}

function AssetCard({
  asset,
  onInsert,
  onDelete,
  deleting,
}: {
  asset: Asset;
  onInsert?: (asset: Asset) => void;
  onDelete: () => void;
  deleting: boolean;
}) {
  const [copied, setCopied] = useState(false);
  const isImage = asset.content_type.startsWith("image/");

  function copyId() {
    void navigator.clipboard?.writeText(asset.id);
    setCopied(true);
    setTimeout(() => setCopied(false), 1200);
  }

  return (
    <div className="group overflow-hidden rounded-md border border-border bg-surface">
      <div className="flex h-24 items-center justify-center border-b border-border bg-surface-2">
        {isImage ? (
          <img src={assetUrl(asset.id)} alt={asset.filename} className="max-h-24 max-w-full object-contain" />
        ) : (
          <FileImage className="size-8 text-muted-foreground" />
        )}
      </div>
      <div className="p-2">
        <p className="truncate text-xs font-medium" title={asset.filename}>
          {asset.filename}
        </p>
        <p className="text-[10px] text-muted-foreground">
          {asset.content_type} · {formatSize(asset.size)}
        </p>
        <div className="mt-1.5 flex items-center gap-1">
          {onInsert && (
            <Button size="sm" variant="secondary" className="h-6 px-2 text-[11px]" onClick={() => onInsert(asset)}>
              Insert
            </Button>
          )}
          <Button
            size="sm"
            variant="ghost"
            className="h-6 px-2 text-[11px]"
            onClick={copyId}
            title="Copy asset id"
          >
            <Copy className="size-3" /> {copied ? "Copied" : "Id"}
          </Button>
          <Button
            size="sm"
            variant="ghost"
            className="ml-auto h-6 px-1.5 text-[11px] text-status-blocked"
            onClick={onDelete}
            disabled={deleting}
            title="Delete asset"
          >
            <Trash2 className="size-3" />
          </Button>
        </div>
      </div>
    </div>
  );
}

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}
