import { useState } from "react";
import {
  Power,
  PowerOff,
  ShieldCheck,
  Trash2,
  Puzzle,
  Layout,
  AlertTriangle,
} from "lucide-react";
import { Card } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import {
  Dialog,
  DialogContent,
  DialogClose,
  DialogTrigger,
} from "@/components/ui/dialog";
import {
  useDeleteExtension,
  useToggleExtension,
} from "@/lib/hooks/use-extensions";
import type { ExtensionRecord } from "@/lib/api/extensions";
import { GrantsDialog } from "./grants-dialog";
import { capLabel, dangerousPairs, sourceLabel } from "./caps";

/** One installed extension: identity, what it provides (exports / frontend),
 *  its capability posture, and the lifecycle actions — enable/disable, edit
 *  grants, uninstall. Enabling is allowed regardless of grants (a no-cap
 *  extension is valid); the card just surfaces the cap state so the operator
 *  reviews before enabling. */
export function ExtensionCard({ ext }: { ext: ExtensionRecord }) {
  const toggle = useToggleExtension();
  const del = useDeleteExtension();
  const [grantsOpen, setGrantsOpen] = useState(false);
  const [delOpen, setDelOpen] = useState(false);

  const warnings = dangerousPairs(ext.granted_caps);
  const grantedCount = ext.granted_caps.length;
  const requestedCount = ext.requested_caps.length;

  return (
    <Card className="flex flex-col gap-3 p-4">
      <div className="flex items-start justify-between gap-2">
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <h3 className="truncate text-sm font-semibold tracking-tight">
              {ext.name}
            </h3>
            <span className="shrink-0 font-mono text-[11px] text-muted-foreground">
              v{ext.version}
            </span>
          </div>
          <span className="block truncate font-mono text-[11px] text-muted-foreground">
            {ext.id}
          </span>
        </div>
        <Badge variant={ext.enabled ? "accent" : "outline"} className="shrink-0">
          {ext.enabled ? "Enabled" : "Disabled"}
        </Badge>
      </div>

      {/* What it provides */}
      <div className="flex flex-wrap gap-1.5">
        {ext.exports.map((e) => (
          <Badge key={e} variant="default">
            <Puzzle className="size-3" /> {e}
          </Badge>
        ))}
        {ext.frontend && (
          <Badge variant="default">
            <Layout className="size-3" /> ui
            {ext.frontend.slots?.length
              ? `: ${ext.frontend.slots.join(", ")}`
              : ""}
          </Badge>
        )}
      </div>

      {/* Capability posture */}
      <div className="flex flex-col gap-1 text-xs text-muted-foreground">
        <span>
          Capabilities:{" "}
          {requestedCount === 0 ? (
            "none requested"
          ) : (
            <span className="text-foreground">
              {grantedCount}/{requestedCount} granted
            </span>
          )}
        </span>
        {grantedCount > 0 && (
          <span className="flex flex-wrap gap-1">
            {ext.granted_caps.map((c) => (
              <span
                key={c}
                className="rounded bg-surface-2 px-1.5 py-0.5 font-mono text-[10px]"
              >
                {capLabel(c)}
              </span>
            ))}
          </span>
        )}
        <span className="truncate text-[11px] text-muted-foreground/70">
          source: {sourceLabel(ext.source)}
        </span>
      </div>

      {warnings.length > 0 && (
        <div className="flex items-start gap-2 rounded-md border border-status-blocked/30 bg-status-blocked/10 px-2.5 py-1.5 text-[11px] text-status-blocked">
          <AlertTriangle className="mt-0.5 size-3.5 shrink-0" />
          <span>Dangerous capability combination granted.</span>
        </div>
      )}

      {/* Actions */}
      <div className="mt-1 flex items-center gap-2 border-t border-border pt-3">
        <Button
          variant={ext.enabled ? "secondary" : "default"}
          size="sm"
          disabled={toggle.isPending}
          onClick={() =>
            toggle.mutate({ id: ext.id, enabled: !ext.enabled })
          }
        >
          {ext.enabled ? <PowerOff /> : <Power />}
          {ext.enabled ? "Disable" : "Enable"}
        </Button>

        <Button
          variant="ghost"
          size="sm"
          onClick={() => setGrantsOpen(true)}
          disabled={requestedCount === 0}
          title={requestedCount === 0 ? "No capabilities requested" : "Edit grants"}
        >
          <ShieldCheck /> Grants
        </Button>

        <div className="ml-auto">
          <Dialog open={delOpen} onOpenChange={setDelOpen}>
            <DialogTrigger asChild>
              <Button variant="ghost" size="icon-sm" title="Uninstall">
                <Trash2 />
              </Button>
            </DialogTrigger>
            <DialogContent
              title={`Uninstall ${ext.id}?`}
              description="Removes the extension record and its grants. The stored .wasm blob is content-addressed and may be shared, so it is not deleted here."
            >
              <div className="mt-2 flex justify-end gap-2">
                <DialogClose asChild>
                  <Button variant="ghost" size="sm">
                    Cancel
                  </Button>
                </DialogClose>
                <Button
                  variant="destructive"
                  size="sm"
                  disabled={del.isPending}
                  onClick={() =>
                    del.mutate(ext.id, { onSuccess: () => setDelOpen(false) })
                  }
                >
                  <Trash2 /> Uninstall
                </Button>
              </div>
            </DialogContent>
          </Dialog>
        </div>
      </div>

      <GrantsDialog ext={ext} open={grantsOpen} onOpenChange={setGrantsOpen} />
    </Card>
  );
}
