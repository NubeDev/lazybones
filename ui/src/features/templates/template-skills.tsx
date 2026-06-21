import { useMemo } from "react";
import { Plus, X } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { useSkills } from "@/lib/hooks/use-skills";
import {
  useAttachToTemplate,
  useDetachFromTemplate,
  useTemplateAttachments,
} from "@/lib/hooks/use-attachments";

const SKILL_KIND = "skill";

/** The "attached skills" control on a template: list the template's skill
 *  attachments, attach more from the skill catalogue, and detach. It speaks the
 *  generic `/templates/:id/attachments` routes with `thing_kind = "skill"`. */
export function TemplateSkills({ templateId }: { templateId: string }) {
  const { data: attached } = useTemplateAttachments(templateId, SKILL_KIND);
  const { data: allSkills } = useSkills();
  const attach = useAttachToTemplate(templateId);
  const detach = useDetachFromTemplate(templateId);

  const attachedIds = useMemo(
    () => new Set((attached ?? []).map((a) => a.thing_id)),
    [attached],
  );

  // Skills not yet attached — the menu of things you can add.
  const available = (allSkills ?? []).filter((s) => !attachedIds.has(s.id));

  // Resolve a thing_id to a skill title for display; fall back to the raw id
  // (a dangling attachment whose skill was deleted still renders its id).
  const titleOf = (id: string) =>
    allSkills?.find((s) => s.id === id)?.title ?? id;

  return (
    <div className="space-y-1">
      <span className="text-xs font-medium">Attached skills</span>
      <div className="flex flex-wrap items-center gap-1.5 rounded-md border border-border bg-surface p-2">
        {(attached ?? []).length === 0 && (
          <span className="text-[11px] text-muted-foreground">
            No skills attached.
          </span>
        )}
        {(attached ?? []).map((a) => (
          <Badge key={a.id} variant="outline" className="gap-1 pr-1">
            <span className="truncate">{titleOf(a.thing_id)}</span>
            <button
              type="button"
              title="Detach skill"
              disabled={detach.isPending}
              onClick={() =>
                detach.mutate({ thingKind: SKILL_KIND, thingId: a.thing_id })
              }
              className="rounded-sm text-muted-foreground hover:text-foreground"
            >
              <X className="size-3" />
            </button>
          </Badge>
        ))}

        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button
              variant="ghost"
              size="sm"
              className="h-6 px-1.5 text-[11px]"
              disabled={available.length === 0 || attach.isPending}
            >
              <Plus className="size-3" /> Attach
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="start">
            {available.map((s) => (
              <DropdownMenuItem
                key={s.id}
                onSelect={() =>
                  attach.mutate({ thingKind: SKILL_KIND, thingId: s.id })
                }
              >
                <span className="truncate">{s.title}</span>
                <span className="ml-2 font-mono text-[10px] text-muted-foreground">
                  {s.id}
                </span>
              </DropdownMenuItem>
            ))}
          </DropdownMenuContent>
        </DropdownMenu>
      </div>
      <span className="block text-[10px] text-muted-foreground">
        Reusable instruction blocks the agent will follow for this recipe.
      </span>
    </div>
  );
}
