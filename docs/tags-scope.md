# Reusable tags

> Status: spec, for approval. Backend-first.
> Audience: whoever implements the tag layer.
> Read [vision.md](vision.md) for the Plan/Run/Task framing first. This doc
> proposes a small, cross-cutting **tag** noun: a reusable `ns:value` label that
> attaches to any entity (task, follow-up, run) via a graph edge.

## The user story this exists to serve

> 1. Tag a task with `repo:iot-gateway` and `issue:abc`.
> 2. Tag a follow-up with the same `repo:iot-gateway`.
> 3. Ask "show me everything carrying `repo:iot-gateway`" and get tasks,
>    follow-ups, and runs back in **one query**.
> 4. Rename `repo:iot-gateway` → `repo:iot` once; every tagged entity follows.

The defining requirement is **reuse and cross-entity reach**: a tag is defined
once and shared, and the same tag spans more than one kind of row.

> A tag **labels**; it does **not route**. Where an entity lives and syncs is a
> separate, single-answer question owned by the `scoped_to` edge — see
> [Tags vs routing](#tags-vs-routing-structure-routes-tags-label) below. This
> distinction is what keeps the team/cloud sync conflict-free
> ([lazybones-server/sync-model.md](lazybones-server/sync-model.md)).

## Decision: a `tag` table + a `tagged_with` RELATION

We are on SurrealDB, so the idiomatic shape is **not** a flat
`(entity, key, value)` side table. It is a graph relation, exactly like the
existing [`depends_on`](../crates/lazybones-store/src/init_schema.rs) and
`learned` relation tables:

- **`tag`** — the reusable label itself, defined once: `{ ns, value }`.
- **`tagged_with`** — a `TYPE RELATION` edge from any entity to a `tag`.

This is the design that fits the codebase's grain. A flat tag table would force
filtered table scans joined back by a hand-maintained string FK and would throw
away traversal — the one thing the store is good at and already leans on.

### Why not the two simpler shapes

We considered and rejected:

- **A flat `tag` table** (`{ entity, key, value }` rows). The relational
  instinct, but the weakest choice here: per-lookup scans, a hand-maintained
  `entity` FK, no traversal, and it re-invents the `(run, entity)` indexing we
  already do by hand everywhere.
- **An embedded `tags: Vec<String>` column** on each row. Zero new tables, zero
  migration (`Option`/empty reads back fine — the pattern used everywhere). Fine
  if tags were per-row freeform labels. But "everything tagged `repo:iot` across
  tasks, follow-ups, and runs" becomes N queries, one per entity type — which
  fails the cross-entity requirement above. We keep this in our back pocket as
  the fallback **only if** tags turn out to be per-task labels with no
  cross-cutting queries.

## The nouns

### `tag` — a reusable label (namespace + value)

Namespace and value are **two columns**, not a parsed `"repo:abc"` string:

| field   | type   | notes                                         |
| ------- | ------ | --------------------------------------------- |
| `id`    | record | auto-minted ULID key, like event/follow-up    |
| `ns`    | string | the key/namespace, e.g. `repo`, `issue`, `iot`|
| `value` | string | the value, e.g. `iot-gateway`, `abc`          |

`UNIQUE(ns, value)` makes a tag reusable: re-filing `repo:iot-gateway` resolves
to the one existing row instead of minting a duplicate. The `repo:iot-gateway`
delimiter form is **display/input sugar only** — we split on the first `:` at the
boundary and store the two columns. Storing the parsed string instead is the part
that bites in six months: values containing `:`, casing drift, and typos creating
near-duplicate tags.

### `tagged_with` — entity → tag (graph edge)

A `TYPE RELATION SCHEMALESS` table. The `in` is any taggable entity
(`task:*`, `follow_up:*`, `run:*`); the `out` is a `tag:*`. The edge is what
makes a single tag span entity types without per-entity columns.

## Schema additions

Appended to [`init_schema.rs`](../crates/lazybones-store/src/init_schema.rs)
`SCHEMA`, idempotent via `IF NOT EXISTS` like every other table:

```sql
DEFINE TABLE IF NOT EXISTS tag SCHEMALESS;
DEFINE FIELD IF NOT EXISTS ns ON tag TYPE string;
DEFINE FIELD IF NOT EXISTS value ON tag TYPE string;
DEFINE INDEX IF NOT EXISTS tag_ns_value ON tag FIELDS ns, value UNIQUE;
DEFINE TABLE IF NOT EXISTS tagged_with TYPE RELATION SCHEMALESS;
DEFINE TABLE IF NOT EXISTS scoped_to TYPE RELATION SCHEMALESS;
```

No migration of existing rows: nothing references tags until something attaches
one.

## Tags vs routing: structure routes, tags label

`tags-scope.md` originally proposed only `tagged_with`. Designing the cloud/edge
team plane ([lazybones-server/](lazybones-server/README.md)) surfaced a second,
adjacent need that **looks** like a tag but must not be one: deciding which
team/user owns an entity, and therefore where it syncs. The long-term decision is
to keep these as **two distinct edges**, because they answer two different kinds of
question:

| Edge | Question | Cardinality | Used for |
| --- | --- | --- | --- |
| **`scoped_to`** | *where does this live / sync?* | **exactly one** | routing to a sync target (an **edge identity**); read permissions |
| **`tagged_with`** | *what is this about?* | **many** | cross-entity discovery, filtering, sharing |

The rule, stated once so no future feature blurs it:

> **Structure routes (one home, typed `scoped_to` edge). Tags label (many,
> freeform `tagged_with`). A tag never decides an entity's write-home.**

Why not route on tags, given the edge already spans entity types? Because a tag is
many-to-many by design — `team:alpha` *and* `team:beta` could both attach to one
task — and "which keyspace owns it?" must have exactly one answer for the sync
model to stay conflict-free. So routing gets its own single-cardinality edge.

`scoped_to` is the same idiomatic `TYPE RELATION` shape; the `in` is any syncable
entity, the `out` is a `team:*` or `user:*`. The store helper enforces
single-cardinality (re-scoping replaces the edge, it does not add a second).

### Where tags *do* touch sync (additive read-shares)

Tags earn a real sync role on the **read** side, where they are safe: a
`share:team-beta` tag grants an *additional* subscribe scope on an entity. Adding
readers is additive and append-only-friendly, so a share-tag can widen visibility
without ever changing the single write-home. Read-scope = tags (additive);
write-home = `scoped_to` (one).

### Tag vocabulary is cloud-authoritative

For the rename story (requirement #4) to hold across a whole team — not just one
laptop — the `tag` **rows** (the shared `ns:value` vocabulary) are
cloud-authoritative: defined once on the team plane, synced down, renamed in one
place so every edge follows. The `tagged_with` **attachments** sync *with* their
entity to its home. This matches the sync rule "collaboration/intent =
cloud-authoritative" ([sync-model.md](lazybones-server/sync-model.md) invariant 1).

## The queries this unlocks

```sql
-- tags on one task
SELECT ->tagged_with->tag.* FROM task:abc;

-- everything (any entity) carrying repo:iot-gateway, in one traversal
SELECT <-tagged_with<-? FROM tag WHERE ns = 'repo' AND value = 'iot-gateway';

-- all repo:* tags, no LIKE needed
SELECT * FROM tag WHERE ns = 'repo';
```

Renaming is one `UPDATE` on the `tag` row; every edge follows automatically.

## Store module

A new `tag/` module mirroring the [`follow_up/`](../crates/lazybones-store/src/follow_up/)
layout, re-exported from `lib.rs` next to `FollowUp`:

```
crates/lazybones-store/src/tag/
  row.rs     — TagRow (RecordId + ns/value) + wire Tag projection
  mod.rs     — module doc + pub use + tests
  attach.rs  — upsert_tag (idempotent on ns,value) + tag_entity (relate edge)
  query.rs   — tags_for(entity), entities_for(ns, value), untag
```

Public surface (shape, names TBD at implementation):

- `upsert_tag(db, ns, value) -> Tag` — get-or-create on `(ns, value)`.
- `tag_entity(db, entity_id, tag_id)` / `untag(db, entity_id, tag_id)` —
  relate / unrelate the edge, idempotent.
- `tags_for(db, entity_id) -> Vec<Tag>`.
- `entities_for(db, ns, value) -> Vec<EntityRef>` — the cross-entity query.

The wire `Tag` leaks no SurrealDB types, exactly like `FollowUp`.

## API & UI (sketch, out of scope for v1 backend)

- `GET /tags`, `POST /entities/{id}/tags`, `DELETE /entities/{id}/tags/{tag}`.
- UI: a tag chip + autocomplete (suggest existing `tag` rows so reuse is the
  default path, not free typing).

These are noted so the seam is visible; the approval ask below is for the
**store layer** only.

## What we are asking to approve

1. The **`tag` + `tagged_with` graph** shape (not a flat table, not embedded
   arrays) as the long-term design.
2. **`ns`/`value` as two columns** with `UNIQUE(ns, value)`; the `repo:abc`
   delimiter is input sugar.
3. Building it as a **`tag/` store module** mirroring `follow_up/`, schema +
   store + tests in the first PR; API/UI in a follow-up.
4. A sibling **`scoped_to` routing edge** (single-cardinality) alongside
   `tagged_with`, with the rule **structure routes, tags label** — so the team
   plane has an unambiguous home per entity without overloading tags.

Open questions for the reviewer:

- Which entities are taggable in v1 — just `task`, or `task` + `follow_up` +
  `run` from the start? (The edge supports all; the question is which UI/API
  surfaces ship first.)
- Should `ns` be a free string, or a small enum/allow-list (`repo`, `issue`,
  …) to prevent namespace sprawl?
