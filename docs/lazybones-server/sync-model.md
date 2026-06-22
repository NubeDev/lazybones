# Sync model — how the edge and the team plane stay consistent

> Status: spec, for approval. The contract every syncing entity must obey.
> Audience: whoever builds the sync boundary and reviews any new syncable noun.
> Parent: [README.md](README.md). Routing/labeling detail: [tags-scope.md](../tags-scope.md).
> **Revised by a peer review** — [review-resolutions.md](review-resolutions.md)
> supersedes conflicting prose here (transport, `scoped_to` subject, id namespacing,
> enforcement, retention). Read it alongside this.

This is the load-bearing doc. "Both servers stay in sync" is a **property** that
holds because of four invariants — **but only if single-writer is *enforced*, not
assumed.** The review showed the original "conflict-free by construction" framing
broke under reassignment and multi-daemon users because nothing *stopped* a second
writer. Convergence is free **once** the enforcement below is in place:

- single-writer on intent is enforced by **cloud `PERMISSIONS`** (an edge cannot
  write `spec`) — [review-resolutions.md](review-resolutions.md) D1;
- single-writer on facts is enforced by routing `scoped_to` to **one edge
  identity** (not a user) + a **two-phase reassignment** handshake — D2, D3;
- ids are **org/edge-namespaced** so two edges' records never collide — D4.

With those, the four invariants below hold. Without them, they don't.

## The four invariants

### 1. Single writer per field (intent vs facts)

Every syncable field is owned by **exactly one plane**:

- **Cloud owns intent**: the task *spec*, who it's assigned to, channels, triggers,
  membership. The cloud writes these; the edge treats them read-only.
- **Edge owns facts**: a task's lifecycle (`claim → running → gating → done`),
  heartbeats, the per-task agent chat. The edge writes these; the cloud mirrors
  them read-only.

Because no field is written by both planes, the merged state is the **union of two
disjoint sets**, never a reconciliation. The cloud never flips a task to
`running`; the edge never reassigns an owner or edits a spec it was handed. This
is the Kubernetes spec-vs-status split, and it is the entire reason this design
needs no conflict resolution.

### 2. Append-only, id-stamped streams

Nothing that crosses the network is mutated in place. State changes are **new
rows** with a stable, sortable id (ULID — the `event` log and the `chat` module
already do exactly this). An append-only log with stable ids is conflict-free by
construction: two participants can write concurrently and the result is a
well-defined interleave by id, with no lost update.

This is what makes **multi-writer team chat safe without CRDTs**: two members
posting offline each append their *own* messages under their *own* ids; on
reconnect the streams interleave by ULID. Nobody edits anyone else's row, so there
is nothing to merge.

Mutable-looking state (a task's *current* status) is a **projection** of its
append-only event stream, computed on read — not a row two planes fight over.

### 3. Routing is structure; labeling is tags

An entity has **exactly one home**, derived from a typed `scoped_to` graph edge
(`entity ->scoped_to-> team | user`). The bridge traverses that one edge to compute
the entity's Zenoh key. One edge → one answer → one write-home → invariant 1 holds.

**Tags never route.** A tag is a freeform, many-to-many label
([tags-scope.md](../tags-scope.md)); if `team:alpha` *and* `team:beta` could both
sit on a task, "which keyspace owns it?" would have two answers and invariant 1
would break. Tags ride along as payload and may grant *additional read shares*
(additive, safe), but the write-home is always the single `scoped_to` edge.

> **Structure routes (one home, typed edge). Tags label (many, freeform).**

### 4. Idempotent, resumable apply

Every apply is an **upsert keyed by the record's stable id**, so re-delivering a
sample (a crash mid-stream, a Zenoh re-alignment) is a no-op, not a duplicate —
the same at-least-once contract the `chat` mirror already honours on
`(task, hcom_id)`. Each side tracks a **cursor** (the last id it has applied per
keyspace) so a reconnecting node resumes from where it stopped rather than
replaying from zero.

## The fabric: Zenoh keyspace

Two key subspaces, partitioned by author, exactly mirroring invariant 1:

The `{user}` segment is the assignee from `scoped_to`; `{project}` comes from the
containment chain ([projects.md](projects.md)).

```text
# CLOUD → EDGE  (intent; cloud is the only publisher)
lazybones/{org}/{team}/{project}/workflow/{wf}/task/{id}/spec    # the assignment
lazybones/{org}/{team}/{project}/{user}/cmd/{cmdId}             # control: pause/cancel/reassign
lazybones/{org}/{team}/{project}/channel/{kind}/{chanId}/msg/*  # chat · workshop · feed items

# EDGE → CLOUD  (facts; the edge is the only publisher)
lazybones/{org}/{team}/{project}/workflow/{wf}/task/{id}/status  # projected current status
lazybones/{org}/{team}/{project}/workflow/{wf}/task/{id}/event/* # the append-only event log
lazybones/{org}/{team}/{project}/{user}/heartbeat              # liveness

# EPHEMERAL  (no storage; lost on disconnect by design)
lazybones/{org}/{team}/{project}/channel/{kind}/{chanId}/presence   # typing / online
```

Subscriptions (scoped by the Zenoh ACLs in [authz.md](authz.md)):

- **Edge daemon (member)** subs the workflows assigned to it (`spec`/`event` under
  its `scoped_to` workflows), its `…/{thisUser}/cmd/*`, and the channels it can read.
- **Cloud storage** subs `lazybones/**/{status,event/**,heartbeat}` and applies to
  the read-model.
- **Manager dashboard** subs `lazybones/{org}/{team}/**/status` for teams it
  manages + `z_get` for the snapshot.

## Durability & offline — why Zenoh, concretely

The hard part of edge sync is the node that was **offline** when something
happened. Zenoh **storages + alignment** solve it without an outbox we hand-roll:

- A **cloud storage** on the spec/cmd keyspace retains assignments. An edge that
  was off when its manager assigned work **aligns on reconnect** and receives the
  missed `spec`.
- An **edge storage** on its own status/event keyspace retains facts produced while
  the cloud was unreachable; alignment carries them up when the link returns.

The keyspace above is the **deferred Zenoh form**. Phase 1 carries the same two
directions over a remote SurrealDB session + an edge `outbox` table
([review-resolutions.md](review-resolutions.md) D1); offline buffering is the
outbox's retry/cursor, not Zenoh storage alignment. Both forms need only invariants
2 and 4 (append-only + idempotent) to replay safely. **Retention is not free** —
`event/*` and channel `msg/*` are unbounded; D7 requires a snapshot+truncate or TTL
policy on both planes.

## The sync bridge — routing-generic, direction-aware

The bridge is **generic over entity type** (the route comes from `scoped_to`, not a
per-table switch) but it **branches on direction**, because intent flows down and
facts flow up — and some nouns (team chat) are multi-writer fan-out, a third
direction class ([review-resolutions.md](review-resolutions.md) D8). The earlier
"no branch at all" claim was wrong.

```text
DOWN (apply intent):  spec sample ─► resolve edge-namespaced id ─► single-task
                      upsert (NOT the bulk workfile importer; D4) ─► scheduler runs

UP   (publish facts): local append (event / task-chat) ─► route via scoped_to(edge)
                      ─► outbox row ─► drain to cloud (cursor = max applied id)

FAN-OUT (team chat):  cloud-authoritative; member posts append up, cloud distributes
                      down to other members — direction per message, not per table
```

Adding a new syncable noun = give it a `scoped_to` edge (→ an edge identity; D2),
an org/edge-namespaced id (D4), and a direction. No per-table bridge code, but the
direction must be declared.

## The convergence guarantee

Given the four invariants **and** the enforcement from
[review-resolutions.md](review-resolutions.md) (D1 cloud PERMISSIONS, D2 edge-id
routing, D3 two-phase reassignment, D4 id namespacing), for any entity:

1. its **intent** fields have one writer (cloud, enforced by PERMISSIONS) → all
   edges that subscribe converge to the cloud's value by idempotent apply;
2. its **fact** fields have one writer (**one edge identity**, enforced by routing
   `scoped_to` to an edge not a user) → the cloud and any manager converge to that
   edge's value by idempotent apply;
3. there is **no field** in both sets, and **no two edges share a record id** (D4)
   → the union is unambiguous;
4. delivery is at-least-once and apply is idempotent + resumable → transient
   disconnects delay convergence but never corrupt it.

Therefore both planes reach the same state once the link is up. The two
counter-examples the review found — a user's second daemon double-running a task,
and an in-place `scoped_to` re-point mid-run — are **closed by the enforcement
above** (edge-identity routing; two-phase, append-only reassignment), not by
assuming they "cannot be constructed." Strip that enforcement and they reappear.

## Review checklist for any new syncable entity

Before a new noun crosses the wire, it must answer:

- [ ] **One writer, enforced?** Which plane owns each field — cloud (intent,
      enforced by PERMISSIONS) or **one edge identity** (fact)? No field written by
      both; facts route to an edge, never a user.
- [ ] **Append-only + org/edge-namespaced id?** Mutations are new id-stamped rows
      (id prefixed by org/edge so two edges never collide — D4); "current" state is
      a projection, not a contested row.
- [ ] **`scoped_to` edge → an edge identity?** Exactly one, so the route is
      derivable. Tags do not route it.
- [ ] **Direction declared?** Intent down, facts up, or fan-out (team chat). The
      bridge is generic over type but not over direction (D8).
- [ ] **Idempotent apply?** Upsert by id; re-delivery is a no-op.
- [ ] **Cursor + retention?** Resumable from the last applied id; a snapshot/TTL
      policy so the append-only stream is bounded (D7).

If all are yes, it syncs with no per-table code and cannot conflict. If any is no,
fix the model before writing the feature.
