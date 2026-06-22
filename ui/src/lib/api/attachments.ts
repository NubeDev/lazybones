import { request } from "./client";
import type { Attachment } from "@/types/skill";

/** The generic attachment routes, scoped to a `template` owner — the first (and
 *  today only) consumer of the polymorphic attachment seam. Mirrors the backend
 *  `/templates/:id/attachments` surface; other owners would get their own client
 *  pointing at their own route prefix. */

/** `GET /templates/:id/attachments?thing_kind=` — a template's attachments. */
export function listTemplateAttachments(
  templateId: string,
  thingKind?: string,
  signal?: AbortSignal,
): Promise<Attachment[]> {
  const q = thingKind ? `?thing_kind=${encodeURIComponent(thingKind)}` : "";
  return request<Attachment[]>(
    `/templates/${encodeURIComponent(templateId)}/attachments${q}`,
    { signal },
  );
}

/** `POST /templates/:id/attachments` — attach a thing. Idempotent. */
export function attachToTemplate(
  templateId: string,
  thingKind: string,
  thingId: string,
): Promise<Attachment> {
  return request<Attachment>(
    `/templates/${encodeURIComponent(templateId)}/attachments`,
    {
      method: "POST",
      auth: true,
      body: { thing_kind: thingKind, thing_id: thingId },
    },
  );
}

/** `DELETE /templates/:id/attachments/:thing_kind/:thing_id` — detach a thing. */
export function detachFromTemplate(
  templateId: string,
  thingKind: string,
  thingId: string,
): Promise<{ deleted: boolean }> {
  return request<{ deleted: boolean }>(
    `/templates/${encodeURIComponent(templateId)}/attachments/${encodeURIComponent(
      thingKind,
    )}/${encodeURIComponent(thingId)}`,
    { method: "DELETE", auth: true },
  );
}
