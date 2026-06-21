import { useState } from "react";
import { Pencil, Trash2 } from "lucide-react";
import { Card } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogTrigger,
  DialogClose,
} from "@/components/ui/dialog";
import { useDeleteSkill } from "@/lib/hooks/use-skills";
import type { Skill } from "@/types/skill";

/** One skill: title, id, description. The whole card opens the full-page editor
 *  via `onOpen`; the trailing buttons edit (same page) and delete-with-confirm. */
export function SkillCard({
  skill,
  onOpen,
}: {
  skill: Skill;
  onOpen: () => void;
}) {
  const del = useDeleteSkill();
  const [open, setOpen] = useState(false);

  return (
    <Card
      role="button"
      tabIndex={0}
      onClick={onOpen}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          onOpen();
        }
      }}
      className="flex cursor-pointer flex-col gap-2 p-4 transition-colors hover:border-accent/40"
    >
      <div className="flex items-start justify-between gap-2">
        <div className="min-w-0">
          <h3 className="truncate text-sm font-semibold tracking-tight">
            {skill.title}
          </h3>
          <span className="font-mono text-[11px] text-muted-foreground">
            {skill.id}
          </span>
        </div>
        <div className="flex shrink-0 items-center" onClick={(e) => e.stopPropagation()}>
          <Button
            variant="ghost"
            size="icon-sm"
            title="Edit skill"
            onClick={onOpen}
          >
            <Pencil />
          </Button>
          <Dialog open={open} onOpenChange={setOpen}>
            <DialogTrigger asChild>
              <Button variant="ghost" size="icon-sm" title="Delete skill">
                <Trash2 />
              </Button>
            </DialogTrigger>
            <DialogContent
              title={`Delete ${skill.id}?`}
              description="This removes the skill. Templates it was attached to are unaffected (the attachment is dropped silently)."
            >
              <div className="mt-2 flex justify-end gap-2">
                <DialogClose asChild>
                  <Button variant="ghost" size="sm">
                    Cancel
                  </Button>
                </DialogClose>
                <Button
                  variant="destructive"
                  size="sm"
                  disabled={del.isPending}
                  onClick={() =>
                    del.mutate(skill.id, { onSuccess: () => setOpen(false) })
                  }
                >
                  <Trash2 /> Delete
                </Button>
              </div>
            </DialogContent>
          </Dialog>
        </div>
      </div>

      {skill.description && (
        <p className="line-clamp-2 text-xs text-muted-foreground">
          {skill.description}
        </p>
      )}
    </Card>
  );
}
