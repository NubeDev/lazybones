# Projects — phase-1 decision record

> Status: ratified. Closes the phase-1 design questions before any code.
> Audience: the implementers of tasks **proj-store**, **proj-api**, **proj-ui**.
> Parents: [projects.md](projects.md) (the shape), [review-resolutions.md](review-resolutions.md)
> (the corrections). Read those first; this file is the settled contract that
> supersedes the "Open questions" in projects.md and pins the schema.

This record resolves the three open questions in [projects.md](projects.md), pins
the exact SurrealDB table/relation definitions to add to
[`crates/lazybones-store/src/init_schema.rs`](../../crates/lazybones-store/src/init_schema.rs),
and states the D4 record-id namespacing rule and where it applies. No application
code lands in this task — downstream tasks build to this contract.

## 1. The three open questions — resolved

### Q1 — A project targets **many** repos, carried as config/tag (not one)

**Decision: a project is repo-agnostic.** A Project is an org/ownership noun — the
containment root the `under`/authz traversal needs ([review-resolutions.md](review-resolutions.md)
D9) — not a technical handle to a single checkout. The repo(s) a project's work
touches are a *property of its workflows/tasks*, recorded as project config and/or
a `repo:*` tag ([../tags-scope.md](../tags-scope.md)), so one project can span a
mono-repo split, a service + its client, or migrate repos over its life without a
schema change. The code shape agrees: tasks already carry repo/worktree targets
per-run, and nothing in the team graph needs a project→repo edge to traverse
visibility. We therefore add **no** `repo` relation; a `repos` config array lives on
the SCHEMALESS `project` row (queried by tag, not by a declared field in phase 1).

### Q2 — A workflow is assigned to exactly **one** edge (single-assignment, no co-run)

**Decision: `scoped_to` stays single-cardinality.** Assignment routes a workflow (or
task) to exactly one **edge identity** ([review-resolutions.md](review-resolutions.md)
D2). Multi-homing a workflow would re-introduce the two-edge-authoritative-writer
hazard the review flagged (risk #1B) and that D2/D3 exist to prevent: two edges
claiming the same task id produce conflicting facts. Collaboration is expressed at
the **project / channel** level (shared visibility, shared chat, many workflows
under one project), never by pointing one workflow at two edges. Reassignment is the
sanctioned way to move work between edges, and it is the append-only two-phase
handoff of D3 — not a second concurrent binding. This keeps invariant 2 (one writer
per routing field) intact by construction.

### Q3 — Roles are **team-level** for v1 (no per-project lead)

**Decision: team-manager granularity is enough for v1.** The three roles stand as in
[projects.md](projects.md): Admin (global flag on `user`), Team Manager and Member
(both per-team, carried on the `member_of` edge's `role` field). A per-project lead
adds a fourth scope and a project-level membership edge for a need no phase-1 flow
has: a manager already sees and assigns across *all* their team's projects, and a
member already works the workflows they're on. Introducing project roles now would
fork authz traversal (team-scoped vs project-scoped) before there is a query that
needs the finer grain. If per-project leads are later required, they slot in as a
`role` on a future `on_project` edge without disturbing the team graph — so deferring
costs nothing. **v1: roles live on `member_of` (team) and the admin flag (global).**

## 2. Schema — exact lines for `init_schema.rs`

Append the block below to the `SCHEMA` const in
[`crates/lazybones-store/src/init_schema.rs`](../../crates/lazybones-store/src/init_schema.rs),
in the same `DEFINE … IF NOT EXISTS` / `SCHEMALESS` idiom as the existing rows. It is
idempotent and additive; it changes no existing table. `edge` and the base table set
overlap the sketch already in [projects.md](projects.md) — this is the authoritative
version. (`document`/`asset`/`branding`/`source` already declare a `project` field;
those stay as-is and simply reference the `project` table minted here.)

```sql
-- Team graph (cloud-side; the org chart the under/authz traversal walks).
DEFINE TABLE IF NOT EXISTS org     SCHEMALESS;
DEFINE TABLE IF NOT EXISTS team    SCHEMALESS;
DEFINE TABLE IF NOT EXISTS user    SCHEMALESS;
DEFINE FIELD IF NOT EXISTS is_admin ON user TYPE bool;          -- global admin flag (Q3)
DEFINE TABLE IF NOT EXISTS project SCHEMALESS;
DEFINE FIELD IF NOT EXISTS status  ON project TYPE string;      -- active | archived
DEFINE TABLE IF NOT EXISTS edge    SCHEMALESS;                  -- a registered daemon identity (D2)

-- Membership: user -> team, role carried on the edge (Q3: team-level roles).
DEFINE TABLE IF NOT EXISTS member_of TYPE RELATION SCHEMALESS;  -- user->team
DEFINE FIELD IF NOT EXISTS role ON member_of TYPE string;       -- manager | member
DEFINE INDEX IF NOT EXISTS member_of_unique ON member_of FIELDS in, out UNIQUE;

-- Containment: the under chain (team->org, project->team, workflow->project).
DEFINE TABLE IF NOT EXISTS under TYPE RELATION SCHEMALESS;
DEFINE INDEX IF NOT EXISTS under_unique ON under FIELDS in, out UNIQUE;
DEFINE INDEX IF NOT EXISTS under_out ON under FIELDS out;       -- "everything under X" traversal

-- Assignment: workflow|task -> edge, single-cardinality (D2 + Q2).
DEFINE TABLE IF NOT EXISTS scoped_to TYPE RELATION SCHEMALESS;
DEFINE INDEX IF NOT EXISTS scoped_to_in_unique ON scoped_to FIELDS in UNIQUE;
DEFINE INDEX IF NOT EXISTS scoped_to_edge ON scoped_to FIELDS out;
```

Notes for the implementer (proj-store):

- **`scoped_to_in_unique` is how single-assignment (Q2) is enforced in storage** —
  one workflow/task (`in`) may have at most one `scoped_to` row. Reassignment (D3)
  is an append-only, id-stamped, latest-wins binding; if the unique index conflicts
  with the two-phase handoff's transient state, relax it to a non-unique index plus
  a "latest row wins" read and record that choice here — do not silently drop the
  single-cardinality guarantee.
- **`member_of.role`** is the only place manager/member lives; admin is the global
  `user.is_admin` bool. Authz enforcement of these is [authz.md](authz.md)
  (SurrealDB `PERMISSIONS`), not this schema — these definitions only make the rows
  and the lookup indices exist.
- **No `repo` table/edge** (Q1): repos ride as project config / `repo:*` tags.
- **No project-level role edge** (Q3): deferred; slots in later without disturbing
  this graph.

## 3. Record-id namespacing (D4) — the rule and where it applies

**Rule (from [review-resolutions.md](review-resolutions.md) D4):** every **syncable**
record id is minted with an **`{org}/{edge}` prefix folded into the record id
itself**, not merely a wire key. A free string like `task:auth` collides the moment
two edges both mint it in the cloud read-model; the prefix makes the id globally
unique at the storage layer, so the cloud chat/read-models keyed on task id stop
conflating two edges' work.

**Where it applies:**

- **Applies — anything that syncs cloud↔edge and is keyed on its id:** `task`, `run`
  (workflow), `event`, `chat`, `hcom_log`, `follow_up`, and the assignment binding
  carried by `scoped_to`. These are the rows two edges can independently produce; the
  `{org}/{edge}` prefix is what keeps them distinct in the cloud.
- **Does *not* apply — cloud-authored, cloud-only graph rows:** `org`, `team`,
  `user`, `project`, `member_of`, `under`. These are authored once on the cloud plane
  (single writer), never minted concurrently by edges, so they keep plain ids. (A
  natural `{org}/…` id is still fine for readability; it is not *required* for
  collision-safety the way the edge-minted rows are.)
- **The edge prefix is the `edge` identity from D2**, not the user — a user owns N
  edges, and the whole point of namespacing is to tell two of one user's daemons
  apart. A **locally-authored** task is not in the team namespace until **promoted**
  (D9); promotion is where the `{org}/{edge}` prefix is applied, and its local
  `scoped_to` (to the local edge) is never synced as intent.

Implementation note: D4 also pins the down-path as a **single-task upsert** with
explicit cross-edge `depends_on` handling — *not* the whole-workfile importer. That
is a proj-store/proj-api concern; flagged here so the id schema and the apply path
are designed together.

## Contract summary (what proj-store / proj-api / proj-ui build to)

| Decision | Settled value |
| --- | --- |
| Project ↔ repo | many; repos as project config / `repo:*` tag — no repo edge |
| Workflow assignment | single edge (`scoped_to` single-cardinality); collaborate at project/channel |
| Roles (v1) | team-level: admin = `user.is_admin`; manager/member = `member_of.role` |
| New tables | `org`, `team`, `user`, `project`, `edge` (SCHEMALESS) |
| New relations | `member_of` (user→team, `role`), `under` (containment), `scoped_to` (workflow\|task→edge) |
| Single-assignment guard | `scoped_to_in_unique` (see D3 caveat for reassignment) |
| Id namespacing (D4) | `{org}/{edge}` prefix on syncable, edge-minted ids; cloud-only graph rows exempt |
