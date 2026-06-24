import { useSyncExternalStore } from "react";
import { CheckCircle2, Info, X, XCircle } from "lucide-react";
import { createObservable } from "@lazybones/ext-sdk";
import type { ExtToast, ToastOptions } from "@lazybones/ext-sdk";
import { cn } from "@/lib/utils/cn";

/** One live toast plus the host-assigned id used to dismiss it. */
interface LiveToast extends ToastOptions {
  id: number;
}

const toasts = createObservable<LiveToast[]>([]);
let seq = 0;
const DEFAULT_DURATION = 5000;

function push(opts: ToastOptions): void {
  const id = ++seq;
  toasts.update((prev) => [...prev, { ...opts, id }]);
  const duration = opts.durationMs ?? DEFAULT_DURATION;
  if (duration > 0) {
    setTimeout(() => dismiss(id), duration);
  }
}

function dismiss(id: number): void {
  toasts.update((prev) => prev.filter((t) => t.id !== id));
}

/** The {@link ExtToast} the host installs into the SDK. Remotes call these; the
 *  host owns rendering via {@link ToastViewport}. */
export const extToast: ExtToast = {
  notify: (opts) => push(opts),
  info: (title, description) => push({ kind: "info", title, description }),
  success: (title, description) => push({ kind: "success", title, description }),
  error: (title, description) => push({ kind: "error", title, description }),
};

const ICONS = {
  info: Info,
  success: CheckCircle2,
  error: XCircle,
} as const;

const ACCENTS = {
  info: "text-accent",
  success: "text-status-done",
  error: "text-status-blocked",
} as const;

/** Renders the live toast stack. Mounted once near the app root by the
 *  extension host provider. Used for both host- and extension-raised toasts. */
export function ToastViewport() {
  const items = useSyncExternalStore(toasts.subscribe, toasts.get, toasts.get);
  if (items.length === 0) return null;
  return (
    <div className="pointer-events-none fixed bottom-4 left-1/2 z-50 flex w-full max-w-sm -translate-x-1/2 flex-col gap-2 px-4">
      {items.map((t) => {
        const kind = t.kind ?? "info";
        const Icon = ICONS[kind];
        return (
          <div
            key={t.id}
            role="status"
            className="pointer-events-auto flex items-start gap-3 rounded-lg border border-border bg-surface p-3 shadow-lg animate-fade-up"
          >
            <Icon className={cn("mt-0.5 size-4 shrink-0", ACCENTS[kind])} />
            <div className="min-w-0 flex-1">
              <p className="text-sm font-medium text-foreground">{t.title}</p>
              {t.description && (
                <p className="mt-0.5 text-xs text-muted-foreground">{t.description}</p>
              )}
            </div>
            <button
              onClick={() => dismiss(t.id)}
              aria-label="Dismiss"
              className="shrink-0 text-muted-foreground transition-colors hover:text-foreground"
            >
              <X className="size-3.5" />
            </button>
          </div>
        );
      })}
    </div>
  );
}
