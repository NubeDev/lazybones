import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  createSkill,
  deleteSkill,
  getSkill,
  listSkills,
  updateSkill,
  type SkillDraft,
} from "@/lib/api/skills";

/** Poll the skill list. */
export function useSkills() {
  return useQuery({
    queryKey: ["skills"],
    queryFn: ({ signal }) => listSkills(signal),
    refetchInterval: 8000,
  });
}

/** Fetch a single skill by id (`GET /skills/:id`). Disabled when `id` is absent
 *  (e.g. the new-skill page), so authoring never fires a doomed request. */
export function useSkill(id?: string) {
  return useQuery({
    queryKey: ["skill", id],
    queryFn: ({ signal }) => getSkill(id as string, signal),
    enabled: id != null,
  });
}

/** Author a new skill (`POST /skills`). */
export function useCreateSkill() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, draft }: { id: string; draft: SkillDraft }) =>
      createSkill(id, draft),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["skills"] }),
  });
}

/** Edit an existing skill (`PUT /skills/:id`). */
export function useUpdateSkill() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, draft }: { id: string; draft: SkillDraft }) =>
      updateSkill(id, draft),
    onSuccess: (_data, { id }) => {
      qc.invalidateQueries({ queryKey: ["skills"] });
      qc.invalidateQueries({ queryKey: ["skill", id] });
    },
  });
}

/** Delete a skill (`DELETE /skills/:id`). */
export function useDeleteSkill() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => deleteSkill(id),
    onSuccess: (_data, id) => {
      qc.invalidateQueries({ queryKey: ["skills"] });
      qc.invalidateQueries({ queryKey: ["skill", id] });
    },
  });
}
