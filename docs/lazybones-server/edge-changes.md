# Edge changes — making `lazybonesd` a team participant

> Status: spec, for approval. Backend-first; changes to the existing daemon.
> Audience: whoever extends the current `lazybonesd` crates.
> Parent: [README.md](README.md). Contract it must obey: [sync-model.md](sync-model.md).
> **Revised by review** — [review-resolutions.md](review-resolutions.md) changes
> this doc's transport (D1: an `outbox` table + remote SurrealDB session, **not** a
> Zenoh bridge, for phase 1), the `scoped_to` subject (D2: an **edge identity**),
> the spec apply (D4: single-task upsert + namespaced ids, not the bulk importer),
> and the identity default (D5: option B). The §3/§6 Zenoh bridge is **deferred**.

Today `lazybonesd` is a single-operator brain: one machine, one `LAZYBONES_LOOP_TOKEN`,
an embedded SurrealDB authoritative for its run (SCOPE.md). To join a team it stays
exactly that — authoritative for the *run* — and gains the seams below to receive
*intent* from the cloud and publish *facts* up. Nothing here weakens the local-first,
restartable property: the edge runs with or without the cloud reachable.

The edge is the **user server**: one developer's daemon, now identified, scoped,
and synced.

## Summary of the seven changes

| # | Change | Crate(s) | Phase |
| --- | --- | --- | --- |
| 1 | Identity & ownership — a user behind the loop | `auth`, `store` | 1 |
| 2 | `scoped_to` routing edge on syncable entities | `store` | 1 |
| 3 | The Zenoh sync bridge | new `lazybones-sync` + `engine` | 1 |
| 4 | Apply received specs (reuse workfile import) | `store`, `engine` | 1 |
| 5 | Generalize `chat` to `(scope, kind)` channels | `store`, `api` | 2 |
| 6 | Edge-side storage + offline buffering | `lazybones-sync` | 2 |
| 7 | Reconcile cloud intent on reconnect | `engine` scheduler | 2 |

## 1. Identity & ownership

The current `ScopedSession` has two principals (loop, agent) and trusts a bearer
token; there is no human identity. For a team, every entity needs an **owner** and
the session needs to know **who** and **which team**.

- Extend `ScopedSession` (`lazybones-auth`) with `owner: UserId` and
  `teams: Vec<(TeamId, Role)>`. `Role` is `Manager | Member` (with a global
  `admin` flag on the user); `for_loop` / `for_agent` stay as the *machine*
  principals, now stamped with the owning user.
- Add a `Role` axis next to `Capability`: a `Manager` session adds
  read-across-team and project creation; `Member` is read-own + drive-own. `can()`
  is unchanged; add a `may_read_team()` check beside the existing `may_act_on()`.
  Full enforcement model (cloud-side) is [authz.md](authz.md); the edge only needs
  enough to stamp ownership and present its JWT.
- The edge **enrolls once** against the cloud (enrollment token → per-user cert +
  JWT, stored in the secret store). The JWT is presented on the Zenoh session; it
  is *not* a new REST auth path — local REST keeps the loop token.
- Stamp `owner` onto `event` rows (the `actor` field already exists — widen it to
  carry the user id) so facts published up are attributable.

> Edge-local REST stays single-operator and trusting. Identity only matters at the
> *network boundary* (what this daemon may pub/sub). Don't over-build local auth.

## 2. `scoped_to` routing edge

Add the one edge the bridge traverses to route any entity (schema in
[tags-scope.md](../tags-scope.md), which now specs both `scoped_to` and `tagged_with`):

```sql
DEFINE TABLE IF NOT EXISTS scoped_to TYPE RELATION SCHEMALESS;
```

- Written when a syncable entity is created: `RELATE workflow:x ->scoped_to-> edge:y`
  — an **edge (daemon) identity, not a user** (a user owns N daemons; assignment
  targets one — [review-resolutions.md](review-resolutions.md) D2). **Exactly one**
  per entity, and re-binding is **append-only latest-wins**, not in-place mutation
  (D3). (Containment — `workflow ->under-> project ->under-> team` — lives cloud-side
  and drives visibility; `scoped_to` is the assignment edge the sync path routes on.)
- The cloud sets it on assignment. A **locally-authored** task scopes to the local
  edge and is **not** synced as intent until promoted, so no field is written by
  both planes (D9).
- This is the *only* thing that decides where an entity syncs. Tags never route
  (invariant 3, [sync-model.md](sync-model.md)).

## 3. The Zenoh sync bridge

A new crate `lazybones-sync` (verb-per-file, ≤400 lines), driven as a Tokio task
inside `lazybonesd` next to the scheduler:

```text
crates/lazybones-sync/src/
  session.rs   — open the Zenoh session (mTLS + JWT), config-driven
  keys.rs      — key-expr builders; derive a key from a scoped_to traversal
  down.rs      — subscribe spec/cmd/channel → hand to the applier
  up.rs        — tail local append-only streams → publish samples
  cursor.rs    — per-keyspace last-applied id; resumable
  mod.rs       — wire it up; tests
```

- **Routing-generic:** `keys.rs` computes a key by traversing `scoped_to`; there is
  no per-table branch. A new syncable noun needs no change here (invariant: one
  bridge for all entities).
- **Up** reuses the live bus the store already publishes on (the same feed `chat`
  append and `event` rows fire) — the bridge is one more subscriber that forwards
  to Zenoh, tagging each sample with the row's ULID for idempotent apply.
- **Down** hands specs to §4 and channel messages to §5.
- The bridge is **optional**: no `[server]` config → no Zenoh session, daemon runs
  exactly as today. This is how local-first is preserved.

## 4. Apply received specs (reuse workfile import)

A cloud assignment is *a workfile authored remotely instead of from disk*. The
edge already imports workfiles via idempotent upsert (`POST /workfile/sync`). The
down-path reuses it:

- The down-path is a **single-task upsert**, not the bulk workfile importer
  ([review-resolutions.md](review-resolutions.md) D4): the importer assumes a
  self-contained workfile with its full `depends_on` graph authored by the loop
  principal, which a single mid-workflow assignment is not. Reuse the importer's
  *upsert primitive*, but spec how cross-edge `depends_on` resolves.
- Ids are **org/edge-namespaced** so two edges' `task:auth` never collide in the
  cloud read-model (D4).
- The scheduler then runs it like any other ready task — no new execution path.
- Apply is idempotent on the namespaced id (invariant 4). The spec is **cloud-owned
  and enforced**: cloud `PERMISSIONS` reject an edge writing intent, so "the edge
  never edits it" is guaranteed, not trusted (invariant 1, D1).

## 5. Generalize `chat` to `(scope, kind)` channels

The existing `chat` table is keyed to a **task** (`chat_task_at` index). Team chat,
workshop, and feed are the same shape at a different scope. Generalize rather than
fork:

- Add `scope` (the channel id) and `kind` (`task | chat | workshop | feed`) to the
  chat row; keep the append-only + idempotent-on-stable-id contract the module
  already has. Migrate the existing per-task chat to `kind = task, scope = task:id`
  (back-compatible: `Option`/default reads fine, the project's standard pattern).
- New index `channel_scope_at` alongside `chat_task_at`.
- `api/routes/`: a `channels.rs` read/append beside the existing `chat.rs`.
- Team channels are **cloud-authoritative**: the edge subscribes to render; a
  member's own posts publish up under their own ulid (multi-writer, conflict-free
  per invariant 2).

> This is the only schema change with a "migration", and it is the no-op kind: a
> new optional column, not a rewrite. Per-task chat keeps working untouched.

## 6. Edge-side storage + offline buffering

To survive the cloud being unreachable (and to backfill on reconnect), give the
edge a **Zenoh storage** on its own up-keyspace and on the down-keyspace it cares
about:

- Facts produced while offline are retained locally and **align up** when the link
  returns; missed specs **align down** on reconnect. This is Zenoh's job, not ours
  (see [sync-model.md](sync-model.md) "Durability & offline").
- The storage backend can be a small rocksdb sidecar or memory+replay; config in
  the `[server]` block. Cursors (§3 `cursor.rs`) make replay idempotent.

## 7. Reconcile cloud intent on reconnect

The scheduler already reconciles against `git worktree list` + `hcom list` on
boot (SCOPE.md principle 3). Extend that pass with the cloud's view:

- After alignment, the set of assigned specs is authoritative for *intent*: a task
  the cloud cancelled (`cmd`) that is still `running` locally gets stopped; a newly
  assigned spec that arrived offline becomes `ready`.
- The edge still owns the *facts*: it reports actual status up; the cloud does not
  override what the worktree truly did. Reconciliation flows intent **down** and
  facts **up**, never the reverse (invariant 1).

## Config

One new optional block (the only new file surface; everything else is in the DB):

```yaml
# lazybones.yaml — absent ⇒ standalone, exactly as today
server:
  router: "tls/cloud.example:7447"   # zenohd endpoint
  enroll_token_env: LAZYBONES_ENROLL  # one-time; exchanged for cert + JWT
  storage_path: ".lazybones/zenoh"    # edge storage for offline buffering
```

No `server:` block → no bridge, no Zenoh, no identity — the daemon is the
single-operator brain it is today. The team plane is strictly additive.

## What we are asking to approve

1. The **seven seams** above as the edge-side scope, gated behind an optional
   `[server]` config so standalone use is untouched.
2. **Reusing the workfile importer** for the spec down-path and the **`chat`
   module** (generalized to channels) for team chat — no parallel subsystems.
3. A **new `lazybones-sync` crate** for the bridge, kept routing-generic via the
   `scoped_to` traversal.

## Open questions

- **Crate boundary** — `lazybones-sync` as its own crate (clean, testable) vs a
  module under `lazybones-engine` (fewer crates)? Recommend its own crate; it has a
  distinct dependency (`zenoh`) the rest should not inherit.
- **Edge storage backend** — rocksdb sidecar vs memory+replay for phase 2? Start
  memory+replay; add rocksdb when offline windows get long.
- **`actor` vs new `owner`** — widen the existing `event.actor` to the user id, or
  add a separate `owner` field? Recommend widening `actor` (it already records who
  acted) and adding `team` for routing.
