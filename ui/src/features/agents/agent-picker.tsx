import { useAgentCatalog } from "@/lib/hooks/use-agent-catalog";

/** Agent + model + effort pickers, driven by the CRUD-able agent catalog.
 *
 *  Shared by the add-task, new-workflow, and new-template dialogs so the three
 *  surfaces stay consistent. `tool`/`model`/`effort` are the selected values
 *  ("" / null = inherit the next level down). The model & effort dropdowns only
 *  appear for an agent that actually offers them. When the agent changes, the
 *  caller-provided `onAgentChange` resets model/effort to that agent's defaults.
 *
 *  Labels are configurable so the workflow/template dialogs can phrase them as
 *  "default" (e.g. "Default agent"). */
export function AgentPicker({
  tool,
  model,
  effort,
  onToolChange,
  onModelChange,
  onEffortChange,
  labels = {},
}: {
  tool: string;
  model: string | null;
  effort: string | null;
  onToolChange: (tool: string) => void;
  onModelChange: (model: string | null) => void;
  onEffortChange: (effort: string | null) => void;
  labels?: { agent?: string; agentHint?: string };
}) {
  const { data: agents } = useAgentCatalog();
  const agent = (agents ?? []).find((a) => a.id === tool.trim()) ?? null;

  return (
    <>
      <Field
        label={labels.agent ?? "Agent"}
        hint={labels.agentHint ?? "blank = inherit default"}
      >
        {agents && agents.length > 0 ? (
          <Select
            value={tool.trim()}
            onChange={(v) => {
              onToolChange(v);
              // Reset model/effort to the newly-selected agent's defaults.
              const next = (agents ?? []).find((a) => a.id === v) ?? null;
              onModelChange(next?.default_model ?? null);
              onEffortChange(next?.default_effort ?? null);
            }}
          >
            <option value="">inherit default</option>
            {agents.map((a) => (
              <option key={a.id} value={a.id}>
                {a.label}
              </option>
            ))}
          </Select>
        ) : (
          <input
            value={tool}
            onChange={(e) => onToolChange(e.target.value)}
            placeholder="claude"
            className="flex h-9 w-full rounded-md border border-border bg-surface-2 px-3 text-xs outline-none transition-colors placeholder:text-muted-foreground/70 focus-visible:border-accent/50 focus-visible:ring-2 focus-visible:ring-ring/40 font-mono"
          />
        )}
      </Field>

      {agent && agent.models.length > 0 && (
        <Field label="Model" hint={`default: ${agent.default_model ?? "—"}`}>
          <Select value={model ?? ""} onChange={(v) => onModelChange(v || null)}>
            <option value="">use agent default</option>
            {agent.models.map((m) => (
              <option key={m} value={m}>
                {m}
              </option>
            ))}
          </Select>
        </Field>
      )}

      {agent && agent.efforts.length > 0 && (
        <Field label="Effort" hint={`default: ${agent.default_effort ?? "—"}`}>
          <Select value={effort ?? ""} onChange={(v) => onEffortChange(v || null)}>
            <option value="">use agent default</option>
            {agent.efforts.map((e) => (
              <option key={e} value={e}>
                {e}
              </option>
            ))}
          </Select>
        </Field>
      )}
    </>
  );
}

function Select({
  value,
  onChange,
  children,
}: {
  value: string;
  onChange: (value: string) => void;
  children: React.ReactNode;
}) {
  return (
    <select
      value={value}
      onChange={(e) => onChange(e.target.value)}
      className="flex h-9 w-full rounded-md border border-border bg-surface-2 px-3 text-xs outline-none transition-colors focus-visible:border-accent/50 focus-visible:ring-2 focus-visible:ring-ring/40 font-mono"
    >
      {children}
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
    <label className="block space-y-1">
      <span className="text-xs font-medium">{label}</span>
      {children}
      {hint && <span className="block text-[10px] text-muted-foreground">{hint}</span>}
    </label>
  );
}
