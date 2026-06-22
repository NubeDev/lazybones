import { useState } from "react";
import { Plus, Sparkles, ServerCrash } from "lucide-react";
import { Topbar } from "@/components/layout/topbar";
import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/empty-state";
import { Skeleton } from "@/components/ui/skeleton";
import { ApiError } from "@/lib/api/client";
import { useSkills } from "@/lib/hooks/use-skills";
import { SkillCard } from "./skill-card";
import { SkillPage } from "./skill-page";

/** The Skills view: a grid of reusable instruction blocks. Selecting one (or
 *  "New skill") opens a full-page editor — no pop-out dialog. */
export function SkillsPage() {
  const { data: skills, isLoading, error } = useSkills();
  // `null` = list; `{ id }` = view/edit that skill; `{ id: undefined }` = author.
  const [open, setOpen] = useState<{ id?: string } | null>(null);

  if (open) {
    return <SkillPage skillId={open.id} onBack={() => setOpen(null)} />;
  }

  const subtitle = skills
    ? `${skills.length} skill${skills.length === 1 ? "" : "s"}`
    : "Loading…";

  const newButton = (
    <Button size="sm" onClick={() => setOpen({})}>
      <Plus /> New skill
    </Button>
  );

  return (
    <div className="flex h-full flex-col">
      <Topbar title="Skills" subtitle={subtitle} actions={newButton} />

      <div className="flex-1 overflow-auto p-5">
        {error ? (
          <EmptyState
            icon={ServerCrash}
            title="Can't load skills"
            description={error instanceof ApiError ? error.message : "Unexpected error"}
          />
        ) : isLoading && !skills ? (
          <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3">
            {Array.from({ length: 4 }).map((_, i) => (
              <Skeleton key={i} className="h-28 w-full" />
            ))}
          </div>
        ) : !skills || skills.length === 0 ? (
          <EmptyState
            icon={Sparkles}
            title="No skills yet"
            description="Author reusable instruction blocks (e.g. code-review-rust, write-tests) once, then attach them to templates."
            action={newButton}
          />
        ) : (
          <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3">
            {skills.map((s) => (
              <SkillCard
                key={s.id}
                skill={s}
                onOpen={() => setOpen({ id: s.id })}
              />
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
