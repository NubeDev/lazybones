import { useState } from "react";
import { Check, FolderGit2, RefreshCw, UploadCloud } from "lucide-react";
import {
  Card,
  CardHeader,
  CardTitle,
  CardDescription,
  CardContent,
} from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import { Separator } from "@/components/ui/separator";
import { Badge } from "@/components/ui/badge";
import { usePreferences, useUpdatePreferences } from "@/lib/hooks/use-preferences";
import {
  useSyncStatus,
  usePullSync,
  usePushSync,
} from "@/lib/hooks/use-content-sync";
import type { SyncConfig } from "@/types/preferences";
import type { SyncState } from "@/types/content-sync";

/** Human label + tone for each sync state. */
const STATE_META: Record<SyncState, { label: string; tone: string }> = {
  unconfigured: { label: "Not configured", tone: "text-muted-foreground" },
  not_checked_out: { label: "Not cloned yet", tone: "text-muted-foreground" },
  synced: { label: "In sync", tone: "text-status-done" },
  ahead: { label: "Local ahead — push", tone: "text-accent" },
  behind: { label: "Behind — pull", tone: "text-status-blocked" },
  diverged: { label: "Diverged", tone: "text-status-blocked" },
  unknown: { label: "Unknown (offline?)", tone: "text-muted-foreground" },
};

/** Example values shown in the form when sync has never been configured, so the
 *  fields are self-documenting (the operator swaps in their own remote). These
 *  are only a starting draft — nothing is saved until "Save" is pressed. */
const EXAMPLE: SyncConfig = {
  enabled: false,
  remote: "me/lazybones-sync",
  branch: "main",
  dir: null,
  auto_push: true,
  auto_pull: true,
};

/** Content-sync settings: configure the git sync repo (remote/branch/dir +
 *  auto push/pull), see the live status, and pull/push on demand. Config is
 *  saved into preferences so it follows the operator across machines. */
export function ContentSyncCard() {
  const prefs = usePreferences();
  const update = useUpdatePreferences();
  const status = useSyncStatus();
  const pull = usePullSync();
  const push = usePushSync();

  // Draft seeded from the saved config once loaded; when nothing is saved yet,
  // prefill the example so the form is self-documenting.
  const saved = prefs.data?.sync ?? EXAMPLE;
  const [draft, setDraft] = useState<SyncConfig | null>(null);
  const cfg = draft ?? saved;
  const set = (patch: Partial<SyncConfig>) => setDraft({ ...cfg, ...patch });

  function save() {
    // An empty remote clears the whole config (the backend drops it too).
    const remote = cfg.remote?.trim() || null;
    update.mutate({ sync: remote ? { ...cfg, remote } : null });
  }

  const meta = STATE_META[status.data?.state ?? "unknown"];
  // Pull/Push act on what's *saved*, so gate them on the persisted remote — not
  // the prefilled example draft.
  const configured = Boolean(prefs.data?.sync?.remote?.trim());

  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <FolderGit2 className="size-4 text-accent" /> Content sync
        </CardTitle>
        <CardDescription>
          Keep your docs, skills, tasks, templates &amp; workflows in a git repo so
          they follow you between machines. Push before you leave one machine, pull
          when you open another.
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        <Field
          label="Remote URL"
          hint="NubeDev/lb-sync · https://github.com/NubeDev/lb-sync · git@github.com:NubeDev/lb-sync.git — all work (authed via gh)."
        >
          <Input
            value={cfg.remote ?? ""}
            onChange={(e) => set({ remote: e.target.value })}
            placeholder="NubeDev/lb-sync"
          />
        </Field>
        <div className="grid grid-cols-2 gap-3">
          <Field label="Branch" hint="default: main">
            <Input
              value={cfg.branch ?? ""}
              onChange={(e) => set({ branch: e.target.value || null })}
              placeholder="main"
            />
          </Field>
          <Field label="Local checkout dir" hint="blank = <data dir>/sync/gh">
            <Input
              value={cfg.dir ?? ""}
              onChange={(e) => set({ dir: e.target.value || null })}
              placeholder="(auto) <data dir>/sync/gh"
            />
          </Field>
        </div>

        <div className="rounded-md border border-border bg-surface-2 p-3 space-y-2.5">
          <Toggle
            checked={cfg.enabled}
            onChange={(v) => set({ enabled: v })}
            label="Enable automatic sync"
            hint="Master switch. When off, sync is manual (the Pull/Push buttons)."
          />
          <div
            className={`space-y-2.5 border-l-2 border-border pl-3 ${
              cfg.enabled ? "" : "pointer-events-none opacity-50"
            }`}
          >
            <Toggle
              checked={cfg.auto_pull}
              onChange={(v) => set({ auto_pull: v })}
              label="Auto-pull on startup"
              hint="Catch up from the remote when the daemon boots."
            />
            <Toggle
              checked={cfg.auto_push}
              onChange={(v) => set({ auto_push: v })}
              label="Auto-push periodically"
              hint="Push your changes to the remote on a timer (every ~2 min)."
            />
          </div>
        </div>

        <Separator />

        <div className="flex items-center justify-between gap-3">
          <div className="flex items-center gap-2 text-xs">
            <span className="text-muted-foreground">Status</span>
            <Badge variant="outline" className={meta.tone}>
              {meta.label}
            </Badge>
            {status.data && (status.data.ahead > 0 || status.data.behind > 0) && (
              <span className="font-mono text-[11px] text-muted-foreground">
                ↑{status.data.ahead} ↓{status.data.behind}
              </span>
            )}
          </div>
          {update.isSuccess && !update.isPending && (
            <span className="inline-flex items-center gap-1 text-xs text-status-done">
              <Check className="size-3.5" /> Saved
            </span>
          )}
        </div>
        {update.isError && (
          <p className="text-xs text-status-failed">{(update.error as Error).message}</p>
        )}

        <div className="flex items-center justify-end gap-2">
          <Button
            variant="ghost"
            size="sm"
            disabled={!configured || pull.isPending}
            onClick={() => pull.mutate()}
            title="Pull from the remote and import"
          >
            <RefreshCw className="size-3.5" /> {pull.isPending ? "Pulling…" : "Pull"}
          </Button>
          <Button
            variant="ghost"
            size="sm"
            disabled={!configured || push.isPending}
            onClick={() => push.mutate()}
            title="Export and push to the remote"
          >
            <UploadCloud className="size-3.5" /> {push.isPending ? "Pushing…" : "Push"}
          </Button>
          <Button onClick={save} size="sm" disabled={update.isPending}>
            {update.isPending ? "Saving…" : "Save"}
          </Button>
        </div>
      </CardContent>
    </Card>
  );
}

function Toggle({
  checked,
  onChange,
  label,
  hint,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
  label: string;
  hint: string;
}) {
  return (
    <label className="flex cursor-pointer items-start gap-2.5">
      <input
        type="checkbox"
        checked={checked}
        onChange={(e) => onChange(e.target.checked)}
        className="mt-0.5 size-4 accent-[var(--color-accent)]"
      />
      <span className="space-y-0.5">
        <span className="block text-xs font-medium">{label}</span>
        <span className="block text-[11px] text-muted-foreground">{hint}</span>
      </span>
    </label>
  );
}

function Field({
  label,
  hint,
  children,
}: {
  label: string;
  hint?: string;
  children: React.ReactNode;
}) {
  return (
    <label className="block space-y-1.5">
      <span className="text-xs font-medium">{label}</span>
      {children}
      {hint && <span className="block text-[11px] text-muted-foreground">{hint}</span>}
    </label>
  );
}
