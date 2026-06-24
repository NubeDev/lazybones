// The task-detail tab body: it renders THIS extension's latest gate verdict for
// the currently-open task, end-to-end:
//
//   UI tab  ──sdk.api──▶  POST /extensions/:id/invoke { export: "gate-check" }
//                              │
//                              ▼   host compiles + runs the WASM guest
//                         gate-check guest returns pass / fail / skip
//                              │
//   render verdict  ◀──────────┘
//
// Everything goes through the SDK handle — the REST client, this extension's own
// id (`sdk.extensionId`, used to address the invoke route), and the SSE stream we
// subscribe to so the verdict refreshes when the task transitions. The panel
// never reaches around the SDK to `fetch` the daemon directly.
import { useCallback, useEffect, useState } from "react";
import type { ExtSdkHandle } from "@lazybones/ext-sdk";

/** Mirror of the daemon `Task` fields this panel reads. */
interface Task {
  id: string;
  title: string;
}

/** Mirror of the daemon `InvokeResponse` (POST /extensions/:id/invoke). */
interface InvokeResponse {
  export: string;
  verdict: { kind: "pass" | "fail" | "skip"; message: string };
  instantiation_micros: number | null;
  faulted: boolean;
}

interface Props {
  sdk: ExtSdkHandle;
  taskId: string;
}

type VerdictKind = InvokeResponse["verdict"]["kind"];

const KIND_STYLE: Record<VerdictKind, { label: string; color: string }> = {
  pass: { label: "PASS", color: "#16a34a" },
  fail: { label: "FAIL", color: "#dc2626" },
  skip: { label: "SKIP", color: "#a16207" },
};

export function GateVerdictPanel({ sdk, taskId }: Props) {
  const [result, setResult] = useState<InvokeResponse | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  // Fetch the task (for a human summary) then ask THIS extension's gate guest to
  // evaluate it. `auth: true` attaches the loop bearer token the guarded invoke
  // route requires.
  const evaluate = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const task = await sdk.api.get<Task>(`/tasks/${taskId}`).catch(() => null);
      const res = await sdk.api.post<InvokeResponse>(
        `/extensions/${sdk.extensionId}/invoke`,
        {
          export: "gate-check",
          input: {
            task_id: taskId,
            task_summary: task?.title ?? taskId,
            // The example evaluates against a representative diff; a real host
            // integration would pass the candidate worktree's rolled-up stats.
            diff: { files_changed: 1, insertions: 0, deletions: 0 },
          },
        },
        { auth: true },
      );
      setResult(res);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  }, [sdk, taskId]);

  // Re-evaluate on mount / task change, and whenever this task transitions.
  useEffect(() => {
    void evaluate();
    const unsub = sdk.events.subscribe<{ task?: string; id?: string }>(
      "transition",
      (ev) => {
        const changed = ev.data?.task ?? ev.data?.id;
        if (!changed || changed === taskId) void evaluate();
      },
    );
    return unsub;
  }, [evaluate, sdk, taskId]);

  return (
    <div style={{ padding: "1rem", fontSize: 14, lineHeight: 1.5 }}>
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 8,
          marginBottom: 8,
        }}
      >
        <strong>Gate verdict</strong>
        {result && <Badge kind={result.verdict.kind} />}
        <button
          onClick={() => void evaluate()}
          disabled={loading}
          style={{ marginLeft: "auto", cursor: "pointer" }}
        >
          {loading ? "Evaluating…" : "Re-run"}
        </button>
      </div>

      {error && (
        <div style={{ color: "#dc2626" }}>Failed to evaluate gate: {error}</div>
      )}

      {!error && !result && !loading && <div>No verdict yet.</div>}

      {result && (
        <>
          <div>{result.verdict.message}</div>
          <div style={{ opacity: 0.7, marginTop: 8, fontSize: 12 }}>
            {result.faulted
              ? "verdict from a fail-closed host fault"
              : `clean guest return${
                  result.instantiation_micros != null
                    ? ` · cold instantiation ${result.instantiation_micros}µs`
                    : ""
                }`}
          </div>
        </>
      )}
    </div>
  );
}

function Badge({ kind }: { kind: VerdictKind }) {
  const { label, color } = KIND_STYLE[kind];
  return (
    <span
      style={{
        color: "#fff",
        background: color,
        borderRadius: 4,
        padding: "1px 6px",
        fontSize: 12,
        fontWeight: 600,
      }}
    >
      {label}
    </span>
  );
}
