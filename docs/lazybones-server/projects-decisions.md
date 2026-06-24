# Projects — phase-1 design decisions (ratified)

> Status: decision record. Closes the phase-1 open questions in
> [projects.md](projects.md) and the blockers in
> [review-resolutions.md](review-resolutions.md) **before any code**.
> Audience: the proj-store / proj-api / proj-ui tasks. This is the contract;
> build to it. Supersedes the "Open questions" prose in projects.md.

This record resolves three things: (1) the three open questions, (2) the exact
SurrealDB table/relation definitions for `crates/lazybones-store/src/init_schema.rs`,
and (3) the D4 id-namespacing rule and where it applies. No application code lands
in this task.

---

## 1. The three open questions

### Q1 — A project targets **many** repos (via config/tag), not one. ✅
A project is an org/ownership noun, not a technical target; binding it to a single
repo would re-create the workflow's repo/worktree concept one level up and force a
new project every time a team's effort spans two repos (common: app + infra). So a
project carries its target(s) as **project config and/or `repo:*` tags**, and the
existing workflow/worktree machinery stays the sole owner of "which checkout a task
runs in." This keeps `project` purely a containment/authz anchor (D9) and adds zero
schema beyond the SCHEMALESS row already holding config. No `repo` field is pinned
on `project`; repos live in the config blob and as `tagged_with`/`repo:*` tags.

### Q2 — A workflow is assigned to **one edge** (single-cardinality `scoped_to`); no co-run. ✅
Multi-homing a workflow re-introduces exactly the two-authoritative-writers hazard
that D2/D3 were written to remove: two edges both LIVE-SELECT the same spec, both
claim it, both write edge-authoritative facts, and the cloud read-model can't pick a
winner. So `scoped_to` stays single-cardinality — one **active** binding per
workflow/task at a time — and collaboration happens at the **project/channel** level
(shared visibility, team chat), never by pointing one workflow at two edges.
Reassignment is the D3 two-phase handoff (release on A, then bind to B), not a second
concurrent binding. (See §2 for why this is enforced by latest-wins read semantics,
not a hard `UNIQUE` index.)

### Q3 — **Team-level** roles for v1; no per-project lead. ✅
The three roles in projects.md (Admin global; Team Manager / Member on `member_of`)
already give a manager authority over **every** project in their team via the `under`
traversal, which covers the v1 goal ("a manager sees their team's work"). A
per-project lead would add a second authz edge (`leads project`) and a precedence
rule between it and team-manager — cost with no v1 payer, since teams are the unit a
manager is scoped to anyway. Defer per-project roles to a later phase; if needed they
slot in as an additional relation without disturbing the team graph below.

---

## 2. Schema additions for `crates/lazybones-store/src/init_schema.rs`

Append these to the `SCHEMA` const, in the existing
`DEFINE … IF NOT EXISTS … SCHEMALESS` / `DEFINE FIELD` / `DEFINE INDEX` idiom (each
line `\n\`-terminated in the Rust string literal, matching the surrounding rows).
All tables are **cloud-only** (the org graph never lands on an edge — an edge knows
only its own identity and its assigned workflows).

```sql
-- ── Org hierarchy (cloud-only graph; projects.md "the team graph") ──
DEFINE TABLE IF NOT EXISTS org     SCHEMALESS;
DEFINE TABLE IF NOT EXISTS team    SCHEMALESS;
DEFINE TABLE IF NOT EXISTS user    SCHEMALESS;
DEFINE FIELD IF NOT EXISTS admin ON user TYPE bool;             -- global admin (manager/member live on member_of)
DEFINE TABLE IF NOT EXISTS project SCHEMALESS;
DEFINE FIELD IF NOT EXISTS status ON project TYPE string;       -- active | archived
DEFINE FIELD IF NOT EXISTS team   ON project TYPE string;       -- denormalized owning team (mirrors `under`)
DEFINE INDEX IF NOT EXISTS project_team ON project FIELDS team;
DEFINE TABLE IF NOT EXISTS edge SCHEMALESS;                     -- a registered daemon identity (D2)
DEFINE FIELD IF NOT EXISTS user ON edge TYPE string;            -- owning user (a user owns N edges)
DEFINE INDEX IF NOT EXISTS edge_user ON edge FIELDS user;

-- ── Membership: user -> team, role on the edge (manager/member, per-team) ──
DEFINE TABLE IF NOT EXISTS member_of TYPE RELATION SCHEMALESS;
DEFINE FIELD IF NOT EXISTS role ON member_of TYPE string;       -- manager | member
DEFINE INDEX IF NOT EXISTS member_of_unique ON member_of FIELDS in, out UNIQUE;

-- ── Containment: team->org, project->team, workflow->project (visibility/authz) ──
DEFINE TABLE IF NOT EXISTS under TYPE RELATION SCHEMALESS;
DEFINE INDEX IF NOT EXISTS under_in_unique ON under FIELDS in UNIQUE;   -- one parent per child
DEFINE INDEX IF NOT EXISTS under_out       ON under FIELDS out;         -- traverse down to a container's children

-- ── Assignment: workflow|task -> edge (D2 edge identity; D3 append-only, latest-wins) ──
DEFINE TABLE IF NOT EXISTS scoped_to TYPE RELATION SCHEMALESS;
DEFINE FIELD IF NOT EXISTS at     ON scoped_to TYPE datetime;   -- id-stamped binding time (latest-wins)
DEFINE FIELD IF NOT EXISTS active ON scoped_to TYPE bool;       -- current binding? (handoff flips old->false)
DEFINE INDEX IF NOT EXISTS scoped_to_in  ON scoped_to FIELDS in;   -- current edge for a workflow/task
DEFINE INDEX IF NOT EXISTS scoped_to_out ON scoped_to FIELDS out;  -- work for an edge (live-query scope, D1)
```

### Why no `UNIQUE` on `scoped_to.in` (the one place the code shape overrides the lean)
projects.md's "single-cardinality" reads at first like a `UNIQUE(in)` guard, and an
earlier draft pinned a `scoped_to_in_unique`. **It is dropped on purpose.** D3 makes
the binding an *append-only, id-stamped, latest-wins row* — reassignment leaves the
released row in place and writes a new one, so a workflow legitimately has **N**
`scoped_to` rows over its life. A hard `UNIQUE(in)` would reject the second
(reassignment) row and break the D3 handoff. Single-cardinality therefore means
**at most one row with `active = true` per `in`**, enforced by the writer
(release-then-bind: flip the old row's `active` to false, then insert the new one)
and read latest-wins by `at`. The non-unique `scoped_to_in` index serves the "who
runs this now?" lookup. Containment (`under`) *does* get `UNIQUE(in)` — a child has
exactly one parent and there is no append-only history there.

### Notes for proj-store
- `member_of` carries `role`; `admin` is a `bool` on `user` (matches projects.md
  "admin is a global flag, manager/member on the edge").
- `project.team` is denormalized alongside `project ->under-> team` so a "projects in
  my team" list is one indexed read without a graph hop; the `under` edge remains the
  authz source of truth.
- All SCHEMALESS, same as every existing table — Rust row types own the full shape;
  only query-critical columns are declared.

---

## 3. D4 record-id namespacing — `{org}/{edge}` prefix

**Rule:** every **syncable** record id is minted with an `{org}/{edge}` prefix folded
into the *record id itself* (not merely a wire/keyspace key), so two edges that both
author `task:auth` become distinct ids in the cloud read-model instead of colliding
(D4 / review risk #3).

**Where it applies:**
- **Syncable run facts** crossing the edge→cloud boundary: `task`, `run`/`workflow`,
  `event`, and the edge-authoritative `chat` rows — i.e. anything drained through the
  `outbox` (D1) or read by the cloud read-model. Their ids carry `{org}/{edge}/…`.
- **`scoped_to`** binding rows are id-stamped (D3) and likewise namespaced, so the
  append-only history is globally unique and latest-wins is unambiguous.
- **Where it does NOT apply:** the org-graph tables here — `org`, `team`, `user`,
  `project`, `edge`, `member_of`, `under` — are **cloud-authored, single-writer**, so
  they keep plain ids (`org:nube`, `team:…`, `project:…`). The prefix exists to
  disambiguate *multi-edge* authorship; the org graph has exactly one author (the
  cloud). The `edge` row's own id is in fact what *supplies* the `{edge}` segment.
- A **locally-authored** task is not namespaced into the team space until **promoted**
  (D9 writer partition); its local `scoped_to` (local edge) is never synced as intent.

Minting is centralized in the store layer (proj-store) so api/ui never hand-build a
namespaced id; the assignment down-path is a **single-task upsert** with explicit
cross-edge `depends_on` handling, not the whole-workfile importer (D4).

---

## Status of the questions this closes
- projects.md "Open questions" (Q1/Q2/Q3) → resolved above (many-repos, single-edge,
  team-level roles).
- review-resolutions.md **D9** (scoped_to writer partition; Project as authz anchor) →
  reflected in §2 (`under` traversal as authz spine, no co-run) and §3 (promotion
  gate).
- Phase-1 **blockers #2** (D2 scoped_to→edge) and **#3** (D4 id namespacing) → closed
  by §2 (`scoped_to → edge`) and §3 (namespacing rule + scope).
- Blockers #1/#4/#5/#6 (transport, reassignment handshake, authz locus, retention) are
  unchanged from review-resolutions.md and out of scope for this design task; they
  gate later phase-1 code, not this schema contract.
