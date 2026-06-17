import type { ReactNode } from "react";
import { ThemeToggle } from "./theme-toggle";
import { ConnectionStatus } from "./connection-status";
import { isDesktop } from "@/lib/utils/platform";

/** The top bar: page title/subtitle on the left, status + actions on the right.
 *  Doubles as the OS drag region when running as a desktop window. */
export function Topbar({
  title,
  subtitle,
  actions,
}: {
  title: string;
  subtitle?: string;
  actions?: ReactNode;
}) {
  return (
    <header
      className={
        "drag-region flex h-14 shrink-0 items-center justify-between gap-4 border-b border-border bg-surface/80 px-5 backdrop-blur"
      }
    >
      <div className="min-w-0 no-drag">
        <h1 className="truncate text-sm font-semibold tracking-tight">{title}</h1>
        {subtitle && (
          <p className="truncate text-xs text-muted-foreground">{subtitle}</p>
        )}
      </div>
      <div className="flex items-center gap-2 no-drag">
        {actions}
        <ConnectionStatus />
        <ThemeToggle />
        {isDesktop() && (
          <span className="hidden text-[10px] font-medium text-muted-foreground/60 sm:inline">
            desktop
          </span>
        )}
      </div>
    </header>
  );
}
