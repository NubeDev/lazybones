import { useRef, useState } from "react";
import { Plus, Upload, Link2, FileUp } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Dialog,
  DialogContent,
  DialogClose,
  DialogTrigger,
} from "@/components/ui/dialog";
import { ApiError } from "@/lib/api/client";
import {
  useInstallFromUrl,
  useUploadExtension,
} from "@/lib/hooks/use-extensions";

type Mode = "upload" | "url";

/** "Install extension" entry point: upload a `.wasm` component or point the
 *  daemon at a URL. New extensions install disabled with no grants (default-deny),
 *  so the operator reviews caps before enabling — this dialog just gets the bytes
 *  in. */
export function InstallDialog() {
  const [open, setOpen] = useState(false);
  const [mode, setMode] = useState<Mode>("upload");
  const [file, setFile] = useState<File | null>(null);
  const [url, setUrl] = useState("");
  const [id, setId] = useState("");
  const fileRef = useRef<HTMLInputElement>(null);

  const upload = useUploadExtension();
  const fromUrl = useInstallFromUrl();
  const pending = upload.isPending || fromUrl.isPending;
  const error = upload.error ?? fromUrl.error;

  function reset() {
    setFile(null);
    setUrl("");
    setId("");
    setMode("upload");
    upload.reset();
    fromUrl.reset();
  }

  function onOpenChange(next: boolean) {
    setOpen(next);
    if (!next) reset();
  }

  function submit() {
    const trimmedId = id.trim() || undefined;
    const done = { onSuccess: () => onOpenChange(false) };
    if (mode === "upload") {
      if (!file) return;
      upload.mutate({ file, id: trimmedId }, done);
    } else {
      if (!url.trim()) return;
      fromUrl.mutate({ url: url.trim(), id: trimmedId }, done);
    }
  }

  const canSubmit = mode === "upload" ? !!file : !!url.trim();

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogTrigger asChild>
        <Button size="sm">
          <Plus /> Install extension
        </Button>
      </DialogTrigger>
      <DialogContent
        title="Install extension"
        description="Upload a .wasm component or fetch one from a URL. It installs disabled with no capabilities granted — review and enable it afterwards."
      >
        <div className="flex flex-col gap-4">
          {/* Mode toggle */}
          <div className="flex gap-1 rounded-md border border-border bg-surface-2 p-1">
            <ModeTab
              active={mode === "upload"}
              icon={Upload}
              label="Upload"
              onClick={() => setMode("upload")}
            />
            <ModeTab
              active={mode === "url"}
              icon={Link2}
              label="From URL"
              onClick={() => setMode("url")}
            />
          </div>

          {mode === "upload" ? (
            <div className="flex flex-col gap-2">
              <button
                type="button"
                onClick={() => fileRef.current?.click()}
                className="flex flex-col items-center justify-center gap-2 rounded-md border border-dashed border-border-strong px-4 py-8 text-center transition-colors hover:border-accent/40"
              >
                <FileUp className="size-5 text-muted-foreground" />
                <span className="text-xs text-muted-foreground">
                  {file ? file.name : "Choose a .wasm file"}
                </span>
                {file && (
                  <span className="text-[11px] text-muted-foreground/70">
                    {(file.size / 1024).toFixed(1)} KB
                  </span>
                )}
              </button>
              <input
                ref={fileRef}
                type="file"
                accept=".wasm,application/wasm"
                className="hidden"
                onChange={(e) => setFile(e.target.files?.[0] ?? null)}
              />
            </div>
          ) : (
            <div className="flex flex-col gap-1.5">
              <label className="text-xs font-medium text-muted-foreground">
                Component URL
              </label>
              <Input
                placeholder="https://example.com/my-ext.wasm"
                value={url}
                onChange={(e) => setUrl(e.target.value)}
              />
            </div>
          )}

          <div className="flex flex-col gap-1.5">
            <label className="text-xs font-medium text-muted-foreground">
              Friendly id <span className="text-muted-foreground/60">(optional)</span>
            </label>
            <Input
              placeholder="defaults to ext-<sha256[:16]>"
              value={id}
              onChange={(e) => setId(e.target.value)}
            />
          </div>

          {error && (
            <p className="text-xs text-status-blocked">
              {error instanceof ApiError ? error.message : "Install failed"}
            </p>
          )}

          <div className="flex justify-end gap-2">
            <DialogClose asChild>
              <Button variant="ghost" size="sm">
                Cancel
              </Button>
            </DialogClose>
            <Button size="sm" disabled={!canSubmit || pending} onClick={submit}>
              {pending ? "Installing…" : "Install"}
            </Button>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}

function ModeTab({
  active,
  icon: Icon,
  label,
  onClick,
}: {
  active: boolean;
  icon: typeof Upload;
  label: string;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={
        "flex flex-1 items-center justify-center gap-1.5 rounded px-3 py-1.5 text-xs font-medium transition-colors " +
        (active
          ? "bg-surface text-foreground shadow-sm"
          : "text-muted-foreground hover:text-foreground")
      }
    >
      <Icon className="size-3.5" /> {label}
    </button>
  );
}
