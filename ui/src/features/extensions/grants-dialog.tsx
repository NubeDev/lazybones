import { useEffect, useState } from "react";
import { ShieldCheck, AlertTriangle } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent } from "@/components/ui/dialog";
import { ApiError } from "@/lib/api/client";
import { useSetGrants } from "@/lib/hooks/use-extensions";
import type { ExtensionRecord } from "@/lib/api/extensions";
import { CAP_INFO, dangerousPairs } from "./caps";

/** Capability grant editor. The admin picks which of the extension's
 *  `requested_caps` are actually granted — the daemon enforces
 *  `granted ⊆ requested`, so the choices are scoped to what the manifest asked
 *  for. Dangerous combinations (e.g. secrets-read + http-fetch) are flagged
 *  louder than any single cap (design §3.3). */
export function GrantsDialog({
  ext,
  open,
  onOpenChange,
}: {
  ext: ExtensionRecord;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const setGrants = useSetGrants();
  const [granted, setGranted] = useState<string[]>(ext.granted_caps);

  // Re-seed when a different extension is opened (or its grants change upstream).
  useEffect(() => {
    if (open) {
      setGranted(ext.granted_caps);
      setGrants.reset();
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, ext.id, ext.granted_caps]);

  const requested = ext.requested_caps;
  const warnings = dangerousPairs(granted);

  function toggle(cap: string) {
    setGranted((prev) =>
      prev.includes(cap) ? prev.filter((c) => c !== cap) : [...prev, cap],
    );
  }

  function save() {
    setGrants.mutate(
      { id: ext.id, grantedCaps: granted },
      { onSuccess: () => onOpenChange(false) },
    );
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        title={`Capabilities — ${ext.name}`}
        description="Default-deny. Grant only the host capabilities this extension needs; it can request, but only an admin allows."
      >
        <div className="flex flex-col gap-3">
          {requested.length === 0 ? (
            <p className="rounded-md border border-dashed border-border-strong px-3 py-6 text-center text-xs text-muted-foreground">
              This extension requests no capabilities.
            </p>
          ) : (
            <ul className="flex flex-col gap-1.5">
              {requested.map((cap) => {
                const info = CAP_INFO[cap];
                const on = granted.includes(cap);
                return (
                  <li key={cap}>
                    <label
                      className={
                        "flex cursor-pointer items-start gap-3 rounded-md border px-3 py-2.5 transition-colors " +
                        (on
                          ? "border-accent/40 bg-accent-soft/30"
                          : "border-border hover:border-border-strong")
                      }
                    >
                      <input
                        type="checkbox"
                        checked={on}
                        onChange={() => toggle(cap)}
                        className="mt-0.5 size-4 accent-accent"
                      />
                      <span className="min-w-0">
                        <span className="block text-sm font-medium">
                          {info?.label ?? cap}{" "}
                          <span className="font-mono text-[11px] text-muted-foreground">
                            {cap}
                          </span>
                        </span>
                        {info?.blurb && (
                          <span className="block text-xs text-muted-foreground">
                            {info.blurb}
                          </span>
                        )}
                      </span>
                    </label>
                  </li>
                );
              })}
            </ul>
          )}

          {warnings.map((w) => (
            <div
              key={w}
              className="flex items-start gap-2 rounded-md border border-status-blocked/30 bg-status-blocked/10 px-3 py-2 text-xs text-status-blocked"
            >
              <AlertTriangle className="mt-0.5 size-4 shrink-0" />
              <span>{w}</span>
            </div>
          ))}

          {setGrants.error && (
            <p className="text-xs text-status-blocked">
              {setGrants.error instanceof ApiError
                ? setGrants.error.message
                : "Could not save grants"}
            </p>
          )}

          <div className="flex justify-end gap-2">
            <Button variant="ghost" size="sm" onClick={() => onOpenChange(false)}>
              Cancel
            </Button>
            <Button size="sm" disabled={setGrants.isPending} onClick={save}>
              <ShieldCheck /> {setGrants.isPending ? "Saving…" : "Save grants"}
            </Button>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}
