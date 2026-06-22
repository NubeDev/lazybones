import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  attachToTemplate,
  detachFromTemplate,
  listTemplateAttachments,
} from "@/lib/api/attachments";

/** Poll a template's attachments, optionally narrowed to one `thing_kind`. */
export function useTemplateAttachments(templateId: string, thingKind?: string) {
  return useQuery({
    queryKey: ["template-attachments", templateId, thingKind ?? null],
    queryFn: ({ signal }) =>
      listTemplateAttachments(templateId, thingKind, signal),
    refetchInterval: 8000,
  });
}

/** Attach a thing to a template (`POST /templates/:id/attachments`). */
export function useAttachToTemplate(templateId: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ thingKind, thingId }: { thingKind: string; thingId: string }) =>
      attachToTemplate(templateId, thingKind, thingId),
    onSuccess: () =>
      qc.invalidateQueries({ queryKey: ["template-attachments", templateId] }),
  });
}

/** Detach a thing from a template (`DELETE /templates/:id/attachments/...`). */
export function useDetachFromTemplate(templateId: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ thingKind, thingId }: { thingKind: string; thingId: string }) =>
      detachFromTemplate(templateId, thingKind, thingId),
    onSuccess: () =>
      qc.invalidateQueries({ queryKey: ["template-attachments", templateId] }),
  });
}
