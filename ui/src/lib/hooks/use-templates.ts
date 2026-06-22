import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  createTemplate,
  deleteTemplate,
  listTemplates,
  updateTemplate,
  type TemplateDraft,
} from "@/lib/api/templates";

/** Poll the template list. */
export function useTemplates() {
  return useQuery({
    queryKey: ["templates"],
    queryFn: ({ signal }) => listTemplates(signal),
    refetchInterval: 8000,
  });
}

/** Author a new template (`POST /templates`). */
export function useCreateTemplate() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, draft }: { id: string; draft: TemplateDraft }) =>
      createTemplate(id, draft),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["templates"] }),
  });
}

/** Edit an existing template (`PUT /templates/:id`). */
export function useUpdateTemplate() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, draft }: { id: string; draft: TemplateDraft }) =>
      updateTemplate(id, draft),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["templates"] }),
  });
}

/** Delete a template (`DELETE /templates/:id`). */
export function useDeleteTemplate() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => deleteTemplate(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["templates"] }),
  });
}
