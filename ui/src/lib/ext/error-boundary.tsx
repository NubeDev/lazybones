import { Component, type ErrorInfo, type ReactNode } from "react";
import { AlertTriangle } from "lucide-react";

interface Props {
  /** The extension id, surfaced in the fallback for triage. */
  extensionId: string;
  /** What broke ("page", "tab", …) — phrased into the fallback copy. */
  label?: string;
  children: ReactNode;
}

interface State {
  error: Error | null;
}

/** Isolates a single remote's render. A broken extension shows a contained
 *  fallback instead of white-screening the whole app (design §4.3 "Failures
 *  isolate to that remote"). Every slot the host renders an extension into is
 *  wrapped in one of these. */
export class ExtErrorBoundary extends Component<Props, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo): void {
    console.error(
      `[ext] extension "${this.props.extensionId}" crashed while rendering` +
        `${this.props.label ? ` (${this.props.label})` : ""}`,
      error,
      info.componentStack,
    );
  }

  render(): ReactNode {
    if (this.state.error) {
      return (
        <div className="m-4 flex items-start gap-3 rounded-lg border border-status-blocked/30 bg-status-blocked/5 p-4 text-sm">
          <AlertTriangle className="mt-0.5 size-4 shrink-0 text-status-blocked" />
          <div className="min-w-0">
            <p className="font-medium text-foreground">
              The “{this.props.extensionId}” extension {this.props.label ?? "view"} failed to render.
            </p>
            <p className="mt-1 break-words text-xs text-muted-foreground">
              {this.state.error.message}
            </p>
            <p className="mt-1 text-xs text-muted-foreground/70">
              The rest of lazybones is unaffected.
            </p>
          </div>
        </div>
      );
    }
    return this.props.children;
  }
}
