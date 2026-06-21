import { useEffect, useState } from "react";
import { ArrowLeft, Pencil, Trash2, X } from "lucide-react";
import { Topbar } from "@/components/layout/topbar";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Markdown } from "@/components/ui/markdown";
import { MarkdownEditor } from "@/components/ui/markdown-editor";
import { Skeleton } from "@/components/ui/skeleton";
import {
  Dialog,
  DialogContent,
  DialogTrigger,
  DialogClose,
} from "@/components/ui/dialog";
import { ApiError } from "@/lib/api/client";
import {
  useCreateSkill,
  useDeleteSkill,
  useSkill,
  useUpdateSkill,
} from "@/lib/hooks/use-skills";
import type { SkillDraft } from "@/lib/api/skills";
import type { Skill } from "@/types/skill";

const EMPTY: SkillDraft = { title: "", description: "", body: "" };

function draftFrom(s: Skill): SkillDraft {
  return { title: s.title, description: s.description ?? "", body: s.body ?? "" };
}

/** A full-page skill editor/viewer — replaces the old pop-out dialog. Pass
 *  `skillId` to open an existing skill (read view with an Edit toggle); pass
 *  nothing (`creating`) to author a new one. The body is edited with a real
 *  markdown editor and rendered with the same renderer when viewing. */
export function SkillPage({
  skillId,
  onBack,
}: {
  skillId?: string;
  onBack: () => void;
}) {
  const creating = skillId == null;
  const { data: skill, isLoading } = useSkill(skillId);

  // `editing` starts true when authoring; for an existing skill it's a toggle.
  const [editing, setEditing] = useState(creating);
  const [id, setId] = useState(skillId ?? "");
  const [draft, setDraft] = useState<SkillDraft>(EMPTY);

  // Hydrate the draft once the skill loads (and whenever we leave edit mode).
  useEffect(() => {
    if (skill && !editing) {
      setId(skill.id);
      setDraft(draftFrom(skill));
    }
  }, [skill, editing]);

  const create = useCreateSkill();
  const update = useUpdateSkill();
  const del = useDeleteSkill();
  const mut = creating ? create : update;

  function startEdit() {
    if (skill) setDraft(draftFrom(skill));
    setEditing(true);
  }

  function cancelEdit() {
    if (creating) {
      onBack();
      return;
    }
    if (skill) setDraft(draftFrom(skill));
    mut.reset();
    setEditing(false);
  }

  function save() {
    const trimmed = id.trim();
    if (!trimmed || !draft.title.trim() || !draft.body.trim()) return;
    mut.mutate(
      { id: trimmed, draft },
      {
        onSuccess: () => {
          if (creating) onBack();
          else setEditing(false);
        },
      },
    );
  }

  const err = mut.error;
  const message =
    err instanceof ApiError
      ? err.status === 409
        ? `A skill "${id.trim()}" already exists.`
        : err.status === 404
          ? `Skill "${id.trim()}" no longer exists.`
          : err.message
      : err
        ? "Something went wrong."
        : null;

  // Loading an existing skill we don't have cached yet.
  if (!creating && !skill && isLoading) {
    return (
      <div className="flex h-full flex-col">
        <Topbar
          title={skillId ?? "Skill"}
          subtitle="Skill"
          actions={
            <Button variant="ghost" size="sm" onClick={onBack}>
              <ArrowLeft /> Back
            </Button>
          }
        />
        <div className="space-y-3 p-6">
          <Skeleton className="h-6 w-48" />
          <Skeleton className="h-4 w-full" />
          <Skeleton className="h-64 w-full" />
        </div>
      </div>
    );
  }

  const canSave =
    !!id.trim() && !!draft.title.trim() && !!draft.body.trim() && !mut.isPending;

  return (
    <div className="flex h-full min-w-0 flex-col">
      <Topbar
        title={creating ? "New skill" : (skill?.title ?? id)}
        subtitle={creating ? "Author a reusable instruction block" : skill?.id}
        actions={
          <div className="flex items-center gap-2">
            <Button variant="ghost" size="sm" onClick={onBack}>
              <ArrowLeft /> Back
            </Button>
            {editing ? (
              <>
                <Button variant="ghost" size="sm" onClick={cancelEdit}>
                  <X /> Cancel
                </Button>
                <Button size="sm" onClick={save} disabled={!canSave}>
                  {creating ? "Create skill" : "Save changes"}
                </Button>
              </>
            ) : (
              <>
                <Button variant="secondary" size="sm" onClick={startEdit}>
                  <Pencil /> Edit
                </Button>
                {skill && (
                  <DeleteSkillButton
                    skill={skill}
                    pending={del.isPending}
                    onConfirm={() =>
                      del.mutate(skill.id, { onSuccess: onBack })
                    }
                  />
                )}
              </>
            )}
          </div>
        }
      />

      <div className="min-h-0 flex-1 overflow-y-auto">
        <div className="mx-auto max-w-3xl p-6">
          {editing ? (
            <div className="space-y-4">
              <Field
                label="Skill id"
                hint={
                  creating
                    ? "lowercase, unique, e.g. code-review-rust"
                    : "the id is fixed once authored"
                }
              >
                <Input
                  value={id}
                  autoFocus={creating}
                  disabled={!creating}
                  onChange={(e) => setId(e.target.value)}
                  placeholder="code-review-rust"
                  className="font-mono"
                />
              </Field>

              <Field label="Title">
                <Input
                  value={draft.title}
                  onChange={(e) => setDraft({ ...draft, title: e.target.value })}
                  placeholder="Rust code review"
                />
              </Field>

              <Field label="Description" hint="shown in the picker (optional)">
                <Input
                  value={draft.description}
                  onChange={(e) =>
                    setDraft({ ...draft, description: e.target.value })
                  }
                  placeholder="How to review Rust changes"
                />
              </Field>

              <Field
                label="Instructions"
                hint="the skill text an agent follows — markdown supported"
              >
                <MarkdownEditor
                  value={draft.body}
                  onChange={(b) => setDraft({ ...draft, body: b })}
                  placeholder="Avoid unwrap in non-test code; add error context; …"
                />
              </Field>

              {message && (
                <p className="rounded-md border border-status-blocked/30 bg-status-blocked/10 px-3 py-2 text-xs text-status-blocked">
                  {message}
                </p>
              )}
            </div>
          ) : (
            <SkillReadView skill={skill} />
          )}
        </div>
      </div>
    </div>
  );
}

/** The read-only render of a skill: title, description, then the body as
 *  rendered markdown. */
function SkillReadView({ skill }: { skill?: Skill }) {
  if (!skill) {
    return <p className="text-sm text-muted-foreground">Skill not found.</p>;
  }
  return (
    <article className="space-y-4">
      {skill.description && (
        <p className="text-sm text-muted-foreground">{skill.description}</p>
      )}
      <div className="rounded-md border border-border bg-surface-2 p-4">
        {skill.body.trim() ? (
          <Markdown>{skill.body}</Markdown>
        ) : (
          <p className="text-xs text-muted-foreground">No instructions yet.</p>
        )}
      </div>
    </article>
  );
}

/** Delete-with-confirm, matching the card's destructive dialog. */
function DeleteSkillButton({
  skill,
  pending,
  onConfirm,
}: {
  skill: Skill;
  pending: boolean;
  onConfirm: () => void;
}) {
  const [open, setOpen] = useState(false);
  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger asChild>
        <Button variant="ghost" size="sm" title="Delete skill">
          <Trash2 /> Delete
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
            disabled={pending}
            onClick={() => {
              setOpen(false);
              onConfirm();
            }}
          >
            <Trash2 /> Delete
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}

function Field({
  label,
  hint,
  children,
}: {
  label: string;
  hint?: string;
  children: React.ReactNode;
}) {
  return (
    <label className="block space-y-1">
      <span className="text-xs font-medium">{label}</span>
      {children}
      {hint && (
        <span className="block text-[10px] text-muted-foreground">{hint}</span>
      )}
    </label>
  );
}
