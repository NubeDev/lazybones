import { useState } from "react";
import {
  Check,
  KeyRound,
  Trash2,
  Terminal,
  CircleCheck,
  CircleSlash,
  Loader2,
  Zap,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { cn } from "@/lib/utils/cn";
import { usePutSecret, useDeleteSecret, useTestAgent } from "@/lib/hooks/use-agents";
import type { AgentReport, SecretMeta } from "@/types/agent";

/** One agent CLI: install state, key entry, and ready/not-ready status. */
export function AgentRow({
  agent,
  secret,
}: {
  agent: AgentReport;
  secret: SecretMeta | undefined;
}) {
  const [value, setValue] = useState("");
  const put = usePutSecret();
  const del = useDeleteSecret();
  const test = useTestAgent();

  function save() {
    if (!value.trim()) return;
    put.mutate(
      { tool: agent.tool, envVar: agent.env_var, value: value.trim() },
      { onSuccess: () => setValue("") },
    );
  }

  return (
    <div className="rounded-lg border border-border bg-surface-2 p-4">
      <div className="flex items-center justify-between gap-3">
        <div className="flex items-center gap-2.5">
          <div
            className={cn(
              "flex size-8 items-center justify-center rounded-md",
              agent.ready ? "bg-status-done/15 text-status-done" : "bg-muted text-muted-foreground",
            )}
          >
            <Terminal className="size-4" />
          </div>
          <div>
            <p className="text-sm font-semibold">{agent.label}</p>
            <code className="text-[11px] text-muted-foreground">{agent.env_var}</code>
          </div>
        </div>
        <ReadyPill agent={agent} />
      </div>

      <div className="mt-3 flex flex-wrap items-center gap-2 text-[11px]">
        <Flag ok={agent.installed} label={agent.installed ? "CLI installed" : "CLI not found"} />
        <Flag
          ok={agent.key_stored || agent.key_in_env}
          label={
            agent.key_stored
              ? `key stored ${secret?.hint ?? ""}`
              : agent.key_in_env
                ? "key in env"
                : "no key"
          }
        />
      </div>

      <div className="mt-3 flex items-center gap-2">
        <div className="relative flex-1">
          <KeyRound className="pointer-events-none absolute left-2.5 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground" />
          <Input
            type="password"
            value={value}
            onChange={(e) => setValue(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && save()}
            placeholder={agent.key_stored ? "Rotate key…" : `Paste ${agent.env_var}…`}
            className="h-8 pl-8 text-xs"
          />
        </div>
        <Button size="sm" onClick={save} disabled={!value.trim() || put.isPending}>
          {put.isSuccess && !value ? <Check /> : null}
          {agent.key_stored ? "Rotate" : "Save"}
        </Button>
        <Button
          size="sm"
          variant="outline"
          onClick={() => test.mutate(agent.tool)}
          disabled={!agent.installed || test.isPending}
          title={
            agent.installed
              ? "Launch the agent with its key to confirm it authenticates"
              : "Install the CLI first"
          }
        >
          {test.isPending ? <Loader2 className="animate-spin" /> : <Zap />}
          Test
        </Button>
        {agent.key_stored && (
          <Button
            size="icon-sm"
            variant="ghost"
            onClick={() => del.mutate(agent.tool)}
            disabled={del.isPending}
            aria-label="Remove key"
          >
            <Trash2 className="text-status-blocked" />
          </Button>
        )}
      </div>

      <TestResult test={test} />

      <p className="mt-2 text-[11px] text-muted-foreground">{agent.login_hint}</p>
    </div>
  );
}

/** Inline outcome of the most recent live credential test. */
function TestResult({ test }: { test: ReturnType<typeof useTestAgent> }) {
  if (test.isPending) {
    return (
      <p className="mt-2 flex items-center gap-1.5 text-[11px] text-muted-foreground">
        <Loader2 className="size-3 animate-spin" /> Launching agent…
      </p>
    );
  }
  if (test.isError) {
    return (
      <p className="mt-2 flex items-center gap-1.5 text-[11px] text-status-blocked">
        <CircleSlash className="size-3" /> {(test.error as Error).message}
      </p>
    );
  }
  if (test.isSuccess) {
    const { ok, detail } = test.data;
    return (
      <p
        className={cn(
          "mt-2 flex items-center gap-1.5 text-[11px]",
          ok ? "text-status-done" : "text-status-blocked",
        )}
      >
        {ok ? <CircleCheck className="size-3" /> : <CircleSlash className="size-3" />}
        {detail}
      </p>
    );
  }
  return null;
}

function ReadyPill({ agent }: { agent: AgentReport }) {
  return (
    <span
      className={cn(
        "inline-flex items-center gap-1 rounded-full border px-2 py-0.5 text-[11px] font-medium",
        agent.ready
          ? "border-status-done/30 bg-status-done/10 text-status-done"
          : "border-border bg-muted text-muted-foreground",
      )}
    >
      {agent.ready ? <CircleCheck className="size-3" /> : <CircleSlash className="size-3" />}
      {agent.ready ? "Ready" : "Not ready"}
    </span>
  );
}

function Flag({ ok, label }: { ok: boolean; label: string }) {
  return (
    <span
      className={cn(
        "inline-flex items-center gap-1 rounded px-1.5 py-0.5",
        ok ? "bg-status-done/10 text-status-done" : "bg-muted text-muted-foreground",
      )}
    >
      <span className="size-1.5 rounded-full" style={{ backgroundColor: "currentColor" }} />
      {label}
    </span>
  );
}
