/** Mirror of `lazybones_store::Skill` — a reusable block of agent instructions. */
export interface Skill {
  id: string;
  title: string;
  description: string;
  body: string;
  created_at: string;
  updated_at: string;
}

/** Mirror of `lazybones_store::Attachment` — a generic owner→thing link.
 *  Both ends are `(kind, id)` strings, so any entity can own any thing-kind. */
export interface Attachment {
  id: string;
  owner_kind: string;
  owner_id: string;
  thing_kind: string;
  thing_id: string;
  created_at: string;
}
