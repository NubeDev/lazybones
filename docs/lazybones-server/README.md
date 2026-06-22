# lazybones-server — the team plane

> Status: spec, for approval. Greenfield (no code yet).
> Audience: whoever builds the cloud server and the edge sync seam.
> Read [vision.md](../vision.md) first (Plan/Run/Task, "lazybones is the durable
> brain"). This proposes the **second plane**: a cloud **team server** that many
> **edge** daemons report to, so a team manager can assign work and watch status
> across everyone's machines.
>
> Companion specs: the org hierarchy and roles in [projects.md](projects.md);
> access control in [authz.md](authz.md); the sync contract in
> [sync-model.md](sync-model.md); the edge-side changes in
> [edge-changes.md](edge-changes.md).
>
> **A peer review revised key decisions** — read
> [review-resolutions.md](review-resolutions.md) first. Most important: phase 1 is
> **remote SurrealDB + an outbox table, not Zenoh** (Zenoh is sequenced for later
> offline/P2P), and `scoped_to` targets an **edge identity, not a user**. The prose
> below that predates those decisions is being reconciled; the resolutions win.

## One sentence

**lazybones-server is the durable, multi-tenant team plane: it owns teams,
membership, and *intent* (assignments, chat, triggers), while each developer's
existing `lazybonesd` stays the authoritative *edge* that actually runs the work
— the two stay in sync over an append-only sync boundary (phase 1: remote
SurrealDB + an outbox table; see [review-resolutions.md](review-resolutions.md)),
never by merging mutable rows.**

Today `lazybonesd` is single-operator: one machine, one loop token, an embedded
SurrealDB that is authoritative for *its* run (SCOPE.md). The team plane does not
replace that. It sits above a fleet of those daemons and adds the one thing they
each lack: a shared, web-reachable place where a team exists, work is handed out,
and status comes home.

## Two planes, one rule

```
  CLOUD — lazybones-server (the team plane)
  ┌──────────────────────────────────────────────────────────┐
  │  server SurrealDB                                          │
  │     org / team / project / member_of   (the team graph)    │
  │     channels: chat · workshop · feed                       │
  │     triggers (feed item → workflow)                        │
  │     read-model: every edge's task status, projected        │
  │     PERMISSIONS enforce single-writer (edge can't write spec)│
  │  manager/admin API + UI (reuse ui/)                        │
  └───────────────▲────────────────────────────▲──────────────┘
   authenticated  │  remote SurrealDB session (TLS + JWT)
   spec ▼ (LIVE)  │  status/event ▲ (outbox drain)   [Zenoh later, see D1]
        ┌──────────┴──────────┐        ┌────────┴───────────┐
        ▼                     ▼         ▼                    ▼
   EDGE — Dev A           Dev B            …                Dev N
   lazybonesd             lazybonesd
   embedded SurrealDB     embedded SurrealDB   ← authoritative for the RUN
   + sync bridge          + sync bridge
```

The rule that keeps the two planes consistent without conflict resolution:

> **Intent & collaboration are cloud-authoritative. Execution facts are
> edge-authoritative. Every record that crosses is append-only and id-stamped.
> No field has two writers, so sync is the *union* of two disjoint streams — never
> a merge.**

| Concern | Authoritative plane | Flows |
| --- | --- | --- |
| org / team / user / membership / **project** | cloud | down (read) |
| workflow / task **spec** / assignment | cloud | down → edge |
| team **chat**, **workshop**, **feed** items | cloud | down (read) |
| **triggers** + what they fired | cloud | internal |
| task **status** / lifecycle **events** | **edge** | up → cloud |
| heartbeats | **edge** | up → cloud |

This is the same spec-vs-status split Kubernetes uses (desired vs observed), and
it is what makes "both stay in sync" a *property*, not a feature you debug. The
full argument and the convergence guarantee are in [sync-model.md](sync-model.md).

## The sync boundary (revised — remote SurrealDB + outbox for phase 1)

> Superseded by [review-resolutions.md](review-resolutions.md) D1. Phase-1 edges
> are developer laptops, mostly online — not intermittent P2P/IoT — so the heavy
> Zenoh fabric is **sequenced for later**, not built now. This aligns with the
> repo's own prior direction
> ([bin/lazyboy/DOCS/IDEA-V3.md](../../bin/lazyboy/DOCS/IDEA-V3.md): "`outbox_events`
> remains the future sync boundary. No Zenoh until the local event model is stable").

- **SurrealDB** is the *brain* at every node. The edge keeps its embedded store
  (local-first, authoritative for run *facts*); the cloud runs a second instance
  (the team graph + read-model).
- The boundary between them is an **append-only `outbox` table + an authenticated
  remote connection**: the edge drains status/event rows up with retry + cursor,
  and receives specs by a LIVE SELECT scoped to its edge id. **Cloud `PERMISSIONS`
  enforce single-writer** — an edge cannot write intent.
- **Zenoh is not the fabric yet.** SurrealDB genuinely has no offline/edge
  replication, so when genuinely-offline / NAT / peer-to-peer edges become a
  requirement, [Zenoh](https://zenoh.io) (or `cr-sqlite` / Matrix per IDEA-V3) is
  evaluated then. The append-only model keeps that swap cheap.

Routing stays derived from the `scoped_to` graph edge (now → an **edge identity**,
not a user; [review-resolutions.md](review-resolutions.md) D2), so the sync path is
generic over entity type but **direction-aware** (intent down / facts up; D8).

## The team graph

A graph, idiomatic to the store (same `TYPE RELATION` shape as `depends_on`,
`learned`, `tagged_with`). The hierarchy is **org → team → project → workflow →
task** ([projects.md](projects.md) is the full spec):

```text
user      ->member_of->  team       # edge carries role: manager | member
team      ->under->      org        # containment
project   ->under->      team       # containment (a project is team-owned)
workflow  ->under->      project    # containment
workflow|task  ->scoped_to->  edge  # assignment: which daemon runs it / where it syncs (D2)
```

Two distinct relationships, and keeping them apart is what keeps sync
conflict-free:

- **Containment (`under`)** drives *visibility/permissions* — a manager traverses
  down their team to see every project, workflow, and task. This is the org chart.
- **Assignment (`scoped_to`)** is the single edge the sync bridge traverses to
  decide *where* a workflow/task runs and syncs. Exactly one, always; tags never
  route (see [tags-scope.md](../tags-scope.md)).

Roles: **Admin** (global), **Team Manager** (per-team, on `member_of`), **Member**
(self). `member_of.role = manager` is the "view team status / create projects"
capability; enforcement is [authz.md](authz.md). The team graph lives **only in
the cloud** — an edge knows its own identity and assigned work, not the org chart.

## Channels — chat, workshop, feed as one primitive

Team chat is not a one-off. It is the first instance of a reusable noun so that
"workshop an idea into a workflow" and "a feed of emails that can trigger a
workflow" are *the same thing*, not three subsystems. (The existing `chat/`
module is per-**task** agent conversation; this generalizes its proven shape —
append-only, idempotent on a stable id, live-published — to a team **scope**.)

**Channel** = a scoped, append-only stream backed by SurrealDB and carried on
Zenoh. Only the `kind` varies:

| kind | what it is | becomes a workflow by |
| --- | --- | --- |
| `task` | the agent conversation you have today | n/a |
| `chat` | a team channel (human↔human; agents may post) | a human promotes a thread |
| `workshop` | an idea thread meant to produce work | a human (or an agent) drafts the task graph → `templates/instantiate` |
| `feed` | ingested external items (email, webhooks, alerts) | a **trigger** matches an item |

**Trigger** = a cloud rule watching a channel that, on a condition, instantiates a
workflow and assigns it. A trigger is just an *automated team manager*: it uses the
exact assignment/spec down-path. Email-feed-fires-a-workflow is one trigger over
one `feed` channel.

So the whole collaboration surface is one pipeline, and new surfaces are new
`kind`s and/or triggers — **not** new plumbing:

```text
 Channel (content) ──► [ human promote │ trigger rule ] ──► Workflow ──► status back
   chat · workshop · feed                                   (tasks+gate)   to the channel
```

Channel **definitions and messages are cloud-authoritative** (collaboration =
cloud). An edge subscribes to render them; a member posting from an edge UI
publishes their own append-only message into the shared channel — multi-writer
but conflict-free, because nobody edits anyone else's message (see the append-only
argument in [sync-model.md](sync-model.md)).

> Channels, triggers, and the workshop→workflow drafting agent are **phase 3–4**.
> They are specced here so the keyspace and the `scoped_to` seam are designed for
> them now; only phases 1–2 (the fabric + assignment + status + team chat) are in
> the first approval ask.

## Identity & authz

Authz is **data-driven**: it falls out of the `member_of` role edges, not
imperative checks. One source of truth, two enforcement points (full spec:
[authz.md](authz.md)):

- **Cloud queries / UI (server SurrealDB):** RECORD access issues the JWT and sets
  `$auth`; `DEFINE TABLE ... PERMISSIONS` scopes every `SELECT` to the caller's
  teams via the graph. SurrealDB is both the identity issuer and the query gate.
- **The wire (Zenoh):** mTLS + key-expression ACLs, minted from the same
  `member_of` edges. A member pub/subs only `…/{thisUser}/**`; a manager adds
  *subscribe* on `lazybones/{org}/{team}/**`; an admin on `lazybones/{org}/**`.

SurrealDB authz gates queries; it does **not** see the Zenoh wire — hence the
second enforcement point. An edge enrolls once (token → cert + JWT) and presents
the JWT on every Zenoh session. The edge's *local* SurrealDB stays trusting; authz
exists only at the network boundary.

## Components (cloud)

Verb-per-file, ≤400 lines, mirroring the edge's `src/api/` grain:

- **server SurrealDB** — the team graph, channels, triggers, and the projected
  read-model. Edges drain their `outbox` (status/event) into it over an
  authenticated remote session; `PERMISSIONS` enforce the intent/facts split.
- **sync boundary** — phase 1: remote SurrealDB session + edge `outbox` table +
  LIVE-SELECT spec delivery. (`zenohd` router(s) are the *deferred* fabric for
  offline/P2P edges — [review-resolutions.md](review-resolutions.md) D1.)
- **enrollment + identity** — issue JWTs (SurrealDB RECORD access), register edge
  identities, drive `PERMISSIONS` from `member_of`.
- **trigger engine** — subscribes feed channels, evaluates rules, instantiates +
  assigns workflows down the spec path.
- **ingest adapters** — email (IMAP/webhook/SES), generic webhook → publish onto a
  `feed` channel. Cloud-side (stable inbox).
- **manager/admin API + UI** — create projects, assign workflows, browse team
  status (Live Query + `z_get` snapshot), team chat. One role-gated UI reusing
  `ui/`, not a second app ([projects.md](projects.md)).

## Phasing

1. **Boundary + assignment round-trip.** Edge registers an identity, opens an
   authenticated remote SurrealDB session; LIVE-SELECTs specs scoped to its edge id
   → single-task upsert; drains `event`/status via the `outbox`. Prove one
   assignment goes cloud → edge A → runs → status returns, on **edge-namespaced ids**
   ([review-resolutions.md](review-resolutions.md) D1–D4).
2. **Durability + team chat.** Outbox retry/cursor + reconnect reconciliation; test
   assign-while-offline. Generalize `chat` to `(scope, kind)` (direction-aware per
   D8); team `chat` channel is the first new kind. Define retention/GC (D7).
3. **Projects + teams + authz + UI.** Team graph (org→team→project→workflow),
   `member_of` roles, SurrealDB RECORD access + `PERMISSIONS`, enrollment/JWT/mTLS,
   Zenoh ACLs, the role-gated Projects/Team/Admin UI sections. ([projects.md](projects.md), [authz.md](authz.md))
4. **Triggers + workshop + feed.** Trigger engine, email ingest adapter, the
   workshop→workflow drafting agent.

## What we are asking to approve

1. The **two-plane split** with the single rule *intent = cloud, facts = edge,
   everything append-only* as the long-term architecture.
2. **Zenoh as the only cross-network fabric**; SurrealDB stays the per-node brain.
   No SurrealDB-to-SurrealDB replication is attempted.
3. **`scoped_to` (typed graph edge) routes; tags label** — the routing/labeling
   split, with [tags-scope.md](../tags-scope.md) updated to match.
4. **Channels + Triggers** as the one extensibility primitive for chat, workshop,
   and feed (built in phases 2–4).
5. **Project → Workflow → Task** hierarchy with three roles, and **SurrealDB
   RECORD access + `PERMISSIONS`** as the cloud authz engine ([projects.md](projects.md),
   [authz.md](authz.md)).

## Open questions

- **Topology** — star (every edge a client of one cloud `zenohd`) to start, or
  router mesh for resilience? Recommend star; revisit at scale.
- **Identity issuer** — self-contained (SurrealDB RECORD auth) vs external OIDC
  (Keycloak/Zenoh-native)? Recommend self-contained for phase 1, OIDC seam later.
  (Detail in [authz.md](authz.md).)
- **Server SurrealDB engine** — embedded `surrealkv` (single cloud node) vs a
  TiKV cluster (HA)? Start embedded; the read-model is rebuildable from the edges.
- **Workshop drafting** — does an agent propose the task graph from the thread, or
  is it human-authored in phase 4? (Leans agent-assisted, reusing `templates`.)
