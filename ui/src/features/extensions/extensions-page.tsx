import { Puzzle, ServerCrash } from "lucide-react";
import { Topbar } from "@/components/layout/topbar";
import { EmptyState } from "@/components/ui/empty-state";
import { Skeleton } from "@/components/ui/skeleton";
import { ApiError } from "@/lib/api/client";
import { useExtensions } from "@/lib/hooks/use-extensions";
import { ExtensionCard } from "./extension-card";
import { InstallDialog } from "./install-dialog";

/** The Extensions view: install (upload / URL), then enable/disable, grant
 *  capabilities, and uninstall WASM + frontend extensions — the management
 *  surface over the daemon `/extensions` API. */
export function ExtensionsPage() {
  const { data: exts, isLoading, error } = useExtensions();

  const subtitle = exts
    ? `${exts.length} extension${exts.length === 1 ? "" : "s"}`
    : "Loading…";

  return (
    <div className="flex h-full flex-col">
      <Topbar title="Extensions" subtitle={subtitle} actions={<InstallDialog />} />

      <div className="flex-1 overflow-auto p-5">
        {error ? (
          <EmptyState
            icon={ServerCrash}
            title="Can't load extensions"
            description={error instanceof ApiError ? error.message : "Unexpected error"}
          />
        ) : isLoading && !exts ? (
          <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3">
            {Array.from({ length: 3 }).map((_, i) => (
              <Skeleton key={i} className="h-44 w-full" />
            ))}
          </div>
        ) : !exts || exts.length === 0 ? (
          <EmptyState
            icon={Puzzle}
            title="No extensions installed"
            description="Extend lazybones without recompiling: install a WASM backend extension (gate checks, event reactions) or a frontend UI extension. They install disabled with no capabilities granted — review and enable each one."
            action={<InstallDialog />}
          />
        ) : (
          <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3">
            {exts.map((ext) => (
              <ExtensionCard key={ext.id} ext={ext} />
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
