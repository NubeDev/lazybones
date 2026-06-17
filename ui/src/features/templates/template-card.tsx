import { useState } from "react";
import { Trash2, Wrench } from "lucide-react";
import { Card } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogTrigger,
  DialogClose,
} from "@/components/ui/dialog";
import { useDeleteTemplate } from "@/lib/hooks/use-templates";
import { WORKTREE_MODES } from "@/features/tasks/worktree-mode";
import type { Template } from "@/types/workflow";

/** One template: title, id, description, default tool/mode, and delete-with-confirm. */
export function TemplateCard({ template }: { template: Template }) {
  const del = useDeleteTemplate();
  const [open, setOpen] = useState(false);

  return (
    <Card className="flex flex-col gap-2 p-4">
      <div className="flex items-start justify-between gap-2">
        <div className="min-w-0">
          <h3 className="truncate text-sm font-semibold tracking-tight">
            {template.title}
          </h3>
          <span className="font-mono text-[11px] text-muted-foreground">
            {template.id}
          </span>
        </div>
        <Dialog open={open} onOpenChange={setOpen}>
          <DialogTrigger asChild>
            <Button variant="ghost" size="icon-sm" title="Delete template">
              <Trash2 />
            </Button>
          </DialogTrigger>
          <DialogContent
            title={`Delete ${template.id}?`}
            description="This removes the recipe. Tasks already created from it are unaffected."
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
                  del.mutate(template.id, { onSuccess: () => setOpen(false) })
                }
              >
                <Trash2 /> Delete
              </Button>
            </div>
          </DialogContent>
        </Dialog>
      </div>

      {template.description && (
        <p className="line-clamp-2 text-xs text-muted-foreground">
          {template.description}
        </p>
      )}

      <div className="mt-auto flex flex-wrap items-center gap-1.5 pt-1">
        {template.default_tool && (
          <Badge variant="outline">
            <Wrench className="size-3" /> {template.default_tool}
          </Badge>
        )}
        {template.default_worktree_mode && (
          <Badge variant="outline">
            {WORKTREE_MODES[template.default_worktree_mode].label}
          </Badge>
        )}
      </div>
    </Card>
  );
}
