import { FileText, ServerCrash } from "lucide-react";
import { Topbar } from "@/components/layout/topbar";
import { EmptyState } from "@/components/ui/empty-state";
import { Skeleton } from "@/components/ui/skeleton";
import { ApiError } from "@/lib/api/client";
import { useTemplates } from "@/lib/hooks/use-templates";
import { TemplateCard } from "./template-card";
import { TemplateDialog } from "./template-dialog";

/** The Templates view: reusable task recipes, picked when adding workflow tasks. */
export function TemplatesPage() {
  const { data: templates, isLoading, error } = useTemplates();

  const subtitle = templates
    ? `${templates.length} template${templates.length === 1 ? "" : "s"}`
    : "Loading…";

  return (
    <div className="flex h-full flex-col">
      <Topbar title="Templates" subtitle={subtitle} actions={<TemplateDialog />} />

      <div className="flex-1 overflow-auto p-5">
        {error ? (
          <EmptyState
            icon={ServerCrash}
            title="Can't load templates"
            description={error instanceof ApiError ? error.message : "Unexpected error"}
          />
        ) : isLoading && !templates ? (
          <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3">
            {Array.from({ length: 4 }).map((_, i) => (
              <Skeleton key={i} className="h-28 w-full" />
            ))}
          </div>
        ) : !templates || templates.length === 0 ? (
          <EmptyState
            icon={FileText}
            title="No templates yet"
            description="Author reusable task recipes (e.g. code-review, open-pr) once, then pick them when adding tasks to a workflow."
            action={<TemplateDialog />}
          />
        ) : (
          <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3">
            {templates.map((t) => (
              <TemplateCard key={t.id} template={t} />
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
