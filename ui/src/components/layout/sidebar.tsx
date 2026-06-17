import { cn } from "@/lib/utils/cn";
import { NAV_ITEMS, type View } from "@/app/navigation";
import { Tooltip } from "@/components/ui/tooltip";

/** The left rail: brand mark + primary navigation. */
export function Sidebar({
  view,
  onNavigate,
}: {
  view: View;
  onNavigate: (v: View) => void;
}) {
  return (
    <aside className="flex w-16 shrink-0 flex-col items-center gap-1 border-r border-border bg-surface py-4 lg:w-56 lg:items-stretch lg:px-3">
      <Brand />
      <nav className="mt-6 flex flex-1 flex-col gap-1">
        {NAV_ITEMS.map((item) => {
          const active = view === item.view;
          const Icon = item.icon;
          return (
            <Tooltip key={item.view} label={item.label} side="right">
              <button
                onClick={() => onNavigate(item.view)}
                className={cn(
                  "group flex items-center gap-3 rounded-md px-3 py-2 text-sm font-medium transition-colors",
                  "justify-center lg:justify-start",
                  active
                    ? "bg-accent-soft/60 text-accent"
                    : "text-muted-foreground hover:bg-muted hover:text-foreground",
                )}
              >
                <Icon className="size-[18px] shrink-0" />
                <span className="hidden lg:inline">{item.label}</span>
              </button>
            </Tooltip>
          );
        })}
      </nav>
      <Footer />
    </aside>
  );
}

function Brand() {
  return (
    <div className="flex items-center gap-2.5 px-1 lg:px-2">
      <div className="flex size-9 shrink-0 items-center justify-center rounded-lg bg-accent text-accent-foreground shadow-sm">
        <span className="font-mono text-sm font-bold">lz</span>
      </div>
      <div className="hidden flex-col lg:flex">
        <span className="text-sm font-semibold leading-none tracking-tight">lazybones</span>
        <span className="text-[10px] leading-tight text-muted-foreground">
          build orchestration
        </span>
      </div>
    </div>
  );
}

function Footer() {
  return (
    <div className="hidden px-2 lg:block">
      <p className="text-[10px] leading-relaxed text-muted-foreground/70">
        many agents · one green gate
      </p>
    </div>
  );
}
