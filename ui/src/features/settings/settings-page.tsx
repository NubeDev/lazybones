import { useMemo, useState } from "react";
import { Bot, Check, Clock, Plug, Search, Sparkles } from "lucide-react";
import { Topbar } from "@/components/layout/topbar";
import { Card, CardHeader, CardTitle, CardDescription, CardContent } from "@/components/ui/card";
import { Tabs, TabsList, TabsTrigger, TabsContent } from "@/components/ui/tabs";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import { Separator } from "@/components/ui/separator";
import { AgentsPanel } from "@/features/agents/agents-panel";
import { ManagementAgentCard } from "./management-agent-card";
import {
  apiBase,
  setApiBase,
  loopToken,
  setLoopToken,
} from "@/lib/api/config";
import {
  isDesktop,
  getTimezone,
  setTimezone,
} from "@/lib/utils/platform";

/** The IANA timezone names for the picker. `Intl.supportedValuesOf` is the
 *  canonical source; fall back to a small set on the rare engine without it. */
function timezoneNames(): string[] {
  const intl = Intl as typeof Intl & {
    supportedValuesOf?: (key: string) => string[];
  };
  if (typeof intl.supportedValuesOf === "function") {
    try {
      return intl.supportedValuesOf("timeZone");
    } catch {
      /* fall through */
    }
  }
  return [
    "UTC",
    "Asia/Ho_Chi_Minh",
    "Asia/Bangkok",
    "Asia/Singapore",
    "Australia/Sydney",
    "Europe/London",
    "America/New_York",
    "America/Los_Angeles",
  ];
}

/** The browser's own zone, shown as the default option's hint. */
function browserZone(): string {
  try {
    return Intl.DateTimeFormat().resolvedOptions().timeZone;
  } catch {
    return "local";
  }
}

/** A searchable timezone picker: a filter box over the full IANA list, plus a
 *  "Follow browser" default that's always offered. The selected value is shown
 *  as the placeholder; the matching list scrolls below. Empty `value` means
 *  follow-browser. */
function TimezonePicker({
  value,
  onChange,
}: {
  value: string;
  onChange: (tz: string) => void;
}) {
  const [query, setQuery] = useState("");

  const followBrowser = `Follow browser (${browserZone()})`;
  const zones = useMemo(() => timezoneNames(), []);
  const matches = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return zones;
    return zones.filter((z) => z.toLowerCase().includes(q));
  }, [zones, query]);

  return (
    <div className="rounded-md border border-border bg-surface">
      <div className="flex items-center gap-2 border-b border-border px-3">
        <Search className="size-3.5 shrink-0 text-muted-foreground" />
        <input
          type="text"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Search timezones…"
          aria-label="Search timezones"
          className="h-9 w-full bg-transparent text-sm outline-none placeholder:text-muted-foreground"
        />
      </div>
      <ul className="max-h-56 overflow-y-auto py-1" role="listbox">
        {/* "Follow browser" default — only shown when it matches the filter. */}
        {(!query.trim() || followBrowser.toLowerCase().includes(query.trim().toLowerCase())) && (
          <TimezoneOption
            label={followBrowser}
            selected={value === ""}
            onSelect={() => onChange("")}
          />
        )}
        {matches.map((name) => (
          <TimezoneOption
            key={name}
            label={name}
            selected={value === name}
            onSelect={() => onChange(name)}
          />
        ))}
        {matches.length === 0 && (
          <li className="px-3 py-2 text-sm text-muted-foreground">No timezones match “{query}”.</li>
        )}
      </ul>
    </div>
  );
}

function TimezoneOption({
  label,
  selected,
  onSelect,
}: {
  label: string;
  selected: boolean;
  onSelect: () => void;
}) {
  return (
    <li role="option" aria-selected={selected}>
      <button
        type="button"
        onClick={onSelect}
        className={`flex w-full items-center justify-between px-3 py-1.5 text-left text-sm transition-colors hover:bg-muted ${
          selected ? "text-accent" : "text-foreground"
        }`}
      >
        {label}
        {selected && <Check className="size-3.5 shrink-0" />}
      </button>
    </li>
  );
}

/** Settings: where lazybonesd lives and the loop token for guarded mutations.
 *  Persisted to localStorage; a save reloads so every query repoints. */
export function SettingsPage() {
  const [base, setBase] = useState(apiBase());
  const [token, setToken] = useState(loopToken());
  const [tz, setTz] = useState(getTimezone() ?? "");
  const [saved, setSaved] = useState(false);

  function save() {
    setApiBase(base.trim());
    setLoopToken(token.trim());
    setTimezone(tz);
    setSaved(true);
    // Repoint all in-flight queries cleanly.
    setTimeout(() => window.location.reload(), 400);
  }

  return (
    <div className="flex h-full flex-col">
      <Topbar title="Settings" subtitle="Connection + environment" />
      <div className="flex-1 overflow-y-auto p-5">
        <div className="mx-auto max-w-2xl">
          <Tabs defaultValue="connection">
            <TabsList className="mb-4">
              <TabsTrigger value="connection">
                <Plug className="size-3.5" /> Connection
              </TabsTrigger>
              <TabsTrigger value="agents">
                <Bot className="size-3.5" /> Agents
              </TabsTrigger>
              <TabsTrigger value="management">
                <Sparkles className="size-3.5" /> Lazybones Agent
              </TabsTrigger>
              <TabsTrigger value="timezone">
                <Clock className="size-3.5" /> Timezone
              </TabsTrigger>
            </TabsList>

            <TabsContent value="connection" className="space-y-4">
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

              <Card>
                <CardHeader>
                  <CardTitle>Environment</CardTitle>
                  <CardDescription>How this client is running.</CardDescription>
                </CardHeader>
                <CardContent className="space-y-2 text-xs">
                  <Row k="Runtime" v={isDesktop() ? "Tauri desktop" : "Browser"} />
                  <Row k="Current API" v={apiBase()} />
                  <Row k="Display timezone" v={getTimezone() ?? `browser (${browserZone()})`} />
                  <Row k="Polling" v="every 4s (no SSE feed yet)" />
                </CardContent>
              </Card>
            </TabsContent>

            <TabsContent value="agents" className="space-y-4">
              <AgentsPanel />
            </TabsContent>

            <TabsContent value="management" className="space-y-4">
              <ManagementAgentCard />
            </TabsContent>

            <TabsContent value="timezone" className="space-y-4">
              <Card>
                <CardHeader>
                  <CardTitle className="flex items-center gap-2">
                    <Clock className="size-4 text-accent" /> Display timezone
                  </CardTitle>
                  <CardDescription>
                    How all dates &amp; times are shown across the app.
                  </CardDescription>
                </CardHeader>
                <CardContent className="space-y-4">
                  <Field
                    label="Display timezone"
                    hint={`"Follow browser" uses this device's zone (${browserZone()}).`}
                  >
                    <TimezonePicker value={tz} onChange={setTz} />
                  </Field>
                  <Separator />
                  <div className="flex items-center justify-end gap-3">
                    {saved && (
                      <span className="inline-flex items-center gap-1 text-xs text-status-done">
                        <Check className="size-3.5" /> Saved — reloading
                      </span>
                    )}
                    <Button onClick={save} size="sm">
                      Save
                    </Button>
                  </div>
                </CardContent>
              </Card>
            </TabsContent>
          </Tabs>
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
