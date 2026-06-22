# Review resolutions — phase-1 design corrections

> Status: decision record. Supersedes conflicting prose in the sibling docs;
> those are being edited to match.
> Audience: anyone implementing phase 1. Read this before the others.

A peer review (verdict: *approve-with-changes on the two-plane concept; rethink
Zenoh-first and single-cardinality `scoped_to` before phase 1*) found that the
"conflict-free by construction" claim did not survive reassignment, multi-daemon
users, and a keyspace/ACL contradiction between the docs. The review was largely
correct and aligned with existing repo direction
([bin/lazyboy/DOCS/IDEA-V3.md](../../bin/lazyboy/DOCS/IDEA-V3.md): "`outbox_events`
remains the future sync boundary. No Zenoh until the local event model is
stable"). Resolutions below.

## D1 — Transport: remote SurrealDB + outbox for phase 1; Zenoh deferred

**Was:** Zenoh as the only cross-network fabric, with storage alignment doing
offline sync, and a dual keyspace + ACL system.

**Now:** Phase-1 edges are developer laptops, mostly online — not intermittent
P2P/IoT nodes. So:

- The **edge keeps its embedded SurrealDB** (local-first, authoritative for run
  *facts* — preserves SCOPE.md principle 3).
- The edge gets an **`outbox` table** (status/event rows to push up), drained to
  the cloud over an **authenticated remote SurrealDB session** with retry + cursor.
- The edge **receives specs** by a LIVE SELECT against the cloud DB, scoped to its
  edge id by `PERMISSIONS` (reuses the existing `/stream` live-query pattern).
- **Single-writer is now enforced, not trusted:** the cloud `PERMISSIONS` reject
  an edge that tries to write `spec` (intent). This dissolves review risk #2 (no
  dual keyspace/ACL) and removes the `lazybones-sync` Zenoh bridge from phase 1.

**Zenoh is not dropped — it is sequenced.** It (or `cr-sqlite`, or Matrix per
IDEA-V3) becomes the replication layer **when genuinely-offline / NAT-traversed /
peer-to-peer edges are a stated requirement**. The append-only event model is what
keeps that swap cheap later. Until then, do not build it.

## D2 — `scoped_to` targets an EDGE identity, not a user (review risk #1B)

A user owns N daemons (laptop + desktop). If assignment routes to a *user* and the
ACL/live-query is per-user, both daemons claim and run the same task → two
edge-authoritative writers. **Fix:** `workflow|task ->scoped_to-> edge` (a
registered daemon identity); a user owns many edges. The spec down-path delivers to
one edge.

## D3 — Reassignment is a two-phase handoff; the binding is append-only (risk #1A)

Re-pointing `scoped_to` in place while edge A is mid-run (and possibly offline)
lets A and B run the same task id concurrently. **Fix:**

1. Cloud marks the workflow `reassigning` and signals edge A to release.
2. Edge A stops, publishes an edge-authoritative `released` fact.
3. Only then does the cloud bind `scoped_to` to edge B.

The binding is an **append-only, id-stamped row (latest-wins)**, not a mutated
edge — so the one field routing depends on still obeys invariant 2.

## D4 — Namespace all syncable ids; single-task upsert, not the bulk importer (risk #3)

Task/run ids are free strings (`task:auth`) and the chat read-model is keyed purely
on task id, ignoring the run ([chat/history.rs](../../crates/lazybones-store/src/chat/history.rs)).
Two edges with `task:auth` collide in the cloud read-model. **Fix:**

- Mint every syncable record id with an `{org}/{edge}` prefix (fold the namespace
  into the *record id*, not just a wire key).
- The assignment down-path is a **single-task upsert** with explicit handling for
  cross-edge `depends_on`, **not** the whole-workfile importer (which assumes a
  self-contained workfile authored by the loop principal — SCOPE.md).

## D5 — Authz default is option B; A needs a connection pool (review: unsound assumption)

Per-request `db.authenticate(jwt)` on a **shared** `Surreal<Db>` handle races —
one request's auth bleeds into another's. **Fix:** default to **option B** (root
session + the existing [guard.rs](../../crates/lazybones-api/src/routes/guard.rs)
enforcing the same clauses), *or* remote authenticated sessions via a
connection-per-request pool. The remote-SurrealDB model (D1) gives pooled
authenticated sessions naturally, so PERMISSIONS enforce directly there.

## D6 — Single source of truth has revocation latency; acknowledge it

PERMISSIONS re-evaluate live; a token/cert outlives a demotion. In the
remote-SurrealDB model this shrinks to "next query" (PERMISSIONS cut immediately),
but tokens still need **short TTL + a refresh/revocation check**. Drop the claim
that the two enforcement points are atomic.

## D7 — Retention / GC for append-only streams (review: unsound assumption)

`event/*` and channel `msg/*` are append-only and currently unbounded on both
planes. **Fix:** define a **snapshot + truncate** policy (periodic status snapshot,
truncate events behind it) or a TTL on cold streams. Tracked as a required phase-2
item, not "the storage layer's job."

## D8 — Chat reuse is a table change, not a generic bridge (review claim #5)

Generalizing `chat` to `(scope, kind)` is sound as a schema change (additive
column + index). But task chat is **edge-authoritative (a fact, up)** while team
chat is **cloud-authoritative multi-writer fan-out (down)** — two sync directions
in one table. So the sync path **branches on direction** (derived from
kind/authority); the earlier "one bridge, no per-table branch" claim is dropped.
The `(task, hcom_id)` dedup is hcom-specific; team chat dedups on its own ULID.

## D9 — `scoped_to` writer partition; Project as authz anchor (review claim #4)

- A **locally-authored** task is not in the team namespace until **promoted**; its
  `scoped_to` (local edge) is never synced as intent. An **assigned** task's
  `scoped_to` is cloud-written. So no field is written by both planes.
- **Project** stays, framed as *the containment root `under`/authz traversal
  needs* — not a domain noun overlapping Run/template. Single-cardinality
  `scoped_to` (no co-run, single repo) was an explicit **routing-model open
  question** — now **resolved** in [projects-decisions.md](projects-decisions.md):
  single edge per workflow (no co-run), and a project spans **many** repos (carried
  as config / `repo:*` tag, not a single-repo binding).

## Phase-1 blockers (must be closed before code)

1. D1 transport decision ratified (remote SurrealDB + outbox; no Zenoh).
2. D2 `scoped_to` → edge identity (decides the routing key shape). **Closed** —
   schema pinned in [projects-decisions.md](projects-decisions.md) §2 (`scoped_to`
   relation, single-cardinality guard).
3. D4 id namespacing (decides record-id schema, not just wire). **Closed** —
   `{org}/{edge}` prefix rule and scope pinned in
   [projects-decisions.md](projects-decisions.md) §3.
4. D3 reassignment handshake defined before any cancel/reassign path.
5. D5 authz enforcement locus (B, or pooled remote sessions).
6. D7 retention policy sketched (can land in phase 2, but named now).

Phase-1 goal is unchanged and reachable — *prove one assignment goes cloud → edge
A → runs → status returns* — but it must be built on D2+D4, or it breaks the moment
a second edge or a reassignment appears.

## Not changed (review agreed or conceded)

- The **two-plane intent/facts split** is sound; it is the *enforcement* that was
  missing, now supplied by D1 (cloud PERMISSIONS) + D2/D3 (edge-identity routing).
- **Append-only + idempotent apply** stands.
- **`scoped_to` routes / tags label** stands (now transport-neutral, not "Zenoh
  keyspace").
