import { useState } from "react";
import { Check, Plug } from "lucide-react";
import { Topbar } from "@/components/layout/topbar";
import { Card, CardHeader, CardTitle, CardDescription, CardContent } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import { Separator } from "@/components/ui/separator";
import { AgentsPanel } from "@/features/agents/agents-panel";
import {
  apiBase,
  setApiBase,
  loopToken,
  setLoopToken,
} from "@/lib/api/config";
import { isDesktop } from "@/lib/utils/platform";

/** Settings: where lazybonesd lives and the loop token for guarded mutations.
 *  Persisted to localStorage; a save reloads so every query repoints. */
export function SettingsPage() {
  const [base, setBase] = useState(apiBase());
  const [token, setToken] = useState(loopToken());
  const [saved, setSaved] = useState(false);

  function save() {
    setApiBase(base.trim());
    setLoopToken(token.trim());
    setSaved(true);
    // Repoint all in-flight queries cleanly.
    setTimeout(() => window.location.reload(), 400);
  }

  return (
    <div className="flex h-full flex-col">
      <Topbar title="Settings" subtitle="Connection + environment" />
      <div className="flex-1 overflow-y-auto p-5">
        <div className="mx-auto max-w-2xl space-y-4">
          <Card>
            <CardHeader>
              <CardTitle className="flex items-center gap-2">
                <Plug className="size-4 text-accent" /> Daemon connection
              </CardTitle>
              <CardDescription>
                The REST address of lazybonesd and the loop bearer token used for
                guarded mutations (promote, claim, sync).
              </CardDescription>
            </CardHeader>
            <CardContent className="space-y-4">
              <Field label="API base URL" hint="e.g. http://127.0.0.1:7878">
                <Input value={base} onChange={(e) => setBase(e.target.value)} />
              </Field>
              <Field label="Loop token" hint="default: lazybones-loop">
                <Input
                  type="password"
                  value={token}
                  onChange={(e) => setToken(e.target.value)}
                />
              </Field>
              <Separator />
              <div className="flex items-center justify-end gap-3">
                {saved && (
                  <span className="inline-flex items-center gap-1 text-xs text-status-done">
                    <Check className="size-3.5" /> Saved — reloading
                  </span>
                )}
                <Button onClick={save} size="sm">
                  Save & reconnect
                </Button>
              </div>
            </CardContent>
          </Card>

          <AgentsPanel />

          <Card>
            <CardHeader>
              <CardTitle>Environment</CardTitle>
              <CardDescription>How this client is running.</CardDescription>
            </CardHeader>
            <CardContent className="space-y-2 text-xs">
              <Row k="Runtime" v={isDesktop() ? "Tauri desktop" : "Browser"} />
              <Row k="Current API" v={apiBase()} />
              <Row k="Polling" v="every 4s (no SSE feed yet)" />
            </CardContent>
          </Card>
        </div>
      </div>
    </div>
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

function Row({ k, v }: { k: string; v: string }) {
  return (
    <div className="flex items-center justify-between">
      <span className="text-muted-foreground">{k}</span>
      <span className="font-mono text-[11px]">{v}</span>
    </div>
  );
}
