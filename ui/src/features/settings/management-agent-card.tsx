import { useEffect, useMemo, useState } from "react";
import { Bot, Check } from "lucide-react";
import {
  Card,
  CardHeader,
  CardTitle,
  CardDescription,
  CardContent,
} from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Separator } from "@/components/ui/separator";
import { ApiError } from "@/lib/api/client";
import { useAgentCatalog } from "@/lib/hooks/use-agent-catalog";
import { useSkills } from "@/lib/hooks/use-skills";
import {
  useManagementAgentConfig,
  useUpdateManagementAgentConfig,
} from "@/lib/hooks/use-management-agent";
import type {
  ManagementAgentDraft,
  PermissionProfile,
  SessionMode,
} from "@/types/management-agent";

/** The "Lazybones Agent" settings card: pick the tool, model, effort, permission
 *  profile, session mode, and which skills the agent may use as runbooks
 *  (`docs/agent/lazybones-agent-scope.md` §5). Phase 1 offers only the
 *  author/read profiles — there is no lifecycle option here. */
export function ManagementAgentCard() {
  const { data: config } = useManagementAgentConfig();
  const { data: catalog } = useAgentCatalog();
  const { data: skills } = useSkills();
  const update = useUpdateManagementAgentConfig();

  const [draft, setDraft] = useState<ManagementAgentDraft | null>(null);
  const [saved, setSaved] = useState(false);

  // Seed the local draft from the server config once it loads.
  useEffect(() => {
    if (config && !draft) {
      setDraft({
        tool: config.tool,
        model: config.model,
        effort: config.effort,
        permission_profile: config.permission_profile,
        session_mode: config.session_mode,
        enabled_skills: config.enabled_skills,
        permission_flags: config.permission_flags,
      });
    }
  }, [config, draft]);

  const tool = useMemo(
    () => catalog?.find((a) => a.id === draft?.tool),
    [catalog, draft?.tool],
  );

  if (!draft) {
    return (
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Bot className="size-4 text-accent" /> Lazybones Agent
          </CardTitle>
          <CardDescription>Loading…</CardDescription>
        </CardHeader>
      </Card>
    );
  }

  const set = (partial: Partial<ManagementAgentDraft>) =>
    setDraft((d) => (d ? { ...d, ...partial } : d));

  const toggleSkill = (id: string) =>
    set({
      enabled_skills: draft.enabled_skills.includes(id)
        ? draft.enabled_skills.filter((s) => s !== id)
        : [...draft.enabled_skills, id],
    });

  const save = () => {
    setSaved(false);
    update.mutate(draft, { onSuccess: () => setSaved(true) });
  };

  const saveErr = update.error instanceof ApiError ? update.error.message : null;

  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <Bot className="size-4 text-accent" /> Lazybones Agent
        </CardTitle>
        <CardDescription>
          A conversational aide that authors workflows, tasks, templates, and
          skills, and explains run state — through the same REST API you use. It
          authors only; on the "Author & manage" profile it can also *propose*
          lifecycle actions, which you confirm before anything runs.
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        <Field label="Tool" hint="The agent CLI the assistant runs as.">
          <Select
            value={draft.tool}
            onChange={(v) => set({ tool: v, model: null, effort: null })}
            options={(catalog ?? []).map((a) => ({ value: a.id, label: a.label }))}
          />
        </Field>

        <div className="grid grid-cols-2 gap-3">
          <Field label="Model" hint="Tool default if unset.">
            <Select
              value={draft.model ?? ""}
              onChange={(v) => set({ model: v || null })}
              options={[
                { value: "", label: "Default" },
                ...(tool?.models ?? []).map((m) => ({ value: m, label: m })),
              ]}
            />
          </Field>
          <Field label="Effort" hint="Tool default if unset.">
            <Select
              value={draft.effort ?? ""}
              onChange={(v) => set({ effort: v || null })}
              options={[
                { value: "", label: "Default" },
                ...(tool?.efforts ?? []).map((e) => ({ value: e, label: e })),
              ]}
            />
          </Field>
        </div>

        <Field
          label="Permission profile"
          hint="What the agent's scoped token may do. It never starts, stops, retries, or deletes anything (Phase 2)."
        >
          <Select
            value={draft.permission_profile}
            onChange={(v) => set({ permission_profile: v as PermissionProfile })}
            options={[
              { value: "author", label: "Author — create/edit + read (default)" },
              {
                value: "author_and_manage",
                label: "Author & manage — + propose lifecycle (you confirm)",
              },
              { value: "read_only", label: "Read only — explain state, never mutate" },
            ]}
          />
        </Field>

        <Field
          label="Session mode"
          hint="Resume one hcom session per conversation (keeps context) or spawn a fresh one each turn."
        >
          <Select
            value={draft.session_mode}
            onChange={(v) => set({ session_mode: v as SessionMode })}
            options={[
              { value: "per_conversation", label: "Per conversation — resume (default)" },
              { value: "per_turn", label: "Per turn — fresh each message" },
            ]}
          />
        </Field>

        <Field
          label="Enabled skills"
          hint="Runbooks the agent may use as operating procedures."
        >
          <div className="flex flex-col gap-1.5 rounded-md border border-border bg-surface p-2">
            {(skills ?? []).length === 0 && (
              <p className="text-[11px] text-muted-foreground">No skills available.</p>
            )}
            {(skills ?? []).map((s) => (
              <label
                key={s.id}
                className="flex cursor-pointer items-start gap-2 text-xs"
              >
                <input
                  type="checkbox"
                  className="mt-0.5"
                  checked={draft.enabled_skills.includes(s.id)}
                  onChange={() => toggleSkill(s.id)}
                />
                <span>
                  <span className="font-medium">{s.title}</span>{" "}
                  <span className="font-mono text-[10px] text-muted-foreground">
                    {s.id}
                  </span>
                </span>
              </label>
            ))}
          </div>
        </Field>

        <Separator />
        <div className="flex items-center justify-end gap-3">
          {saveErr && <span className="text-xs text-status-blocked">{saveErr}</span>}
          {saved && !saveErr && (
            <span className="inline-flex items-center gap-1 text-xs text-status-done">
              <Check className="size-3.5" /> Saved
            </span>
          )}
          <Button onClick={save} size="sm" disabled={update.isPending}>
            Save
          </Button>
        </div>
      </CardContent>
    </Card>
  );
}

/** A minimal styled native select, matching the settings page's plain-HTML idiom. */
function Select({
  value,
  onChange,
  options,
}: {
  value: string;
  onChange: (v: string) => void;
  options: { value: string; label: string }[];
}) {
  return (
    <select
      value={value}
      onChange={(e) => onChange(e.target.value)}
      className="h-9 w-full rounded-md border border-border bg-surface px-2.5 text-sm outline-none focus:border-border-strong"
    >
      {options.map((o) => (
        <option key={o.value} value={o.value}>
          {o.label}
        </option>
      ))}
    </select>
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
