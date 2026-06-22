# Projects — the long-running container above workflows

> Status: spec, for approval. The org hierarchy and the one-UI plan.
> Audience: whoever builds the team graph, the project routes, and the UI sections.
> Parent: [README.md](README.md). Authz that gates this: [authz.md](authz.md).

A team's work is not one workflow — it is a *stream* of workflows over months
against the same long-running effort. That effort needs a durable home above the
workflow. That home is a **Project**.

> Framing (per review): Project earns its place as **the containment root the
> `under`/authz traversal needs** — the anchor for "a manager sees their team's
> work" — not as a domain noun competing with Run/template. And single-cardinality
> `scoped_to` (no co-run, one repo) is an explicit **routing-model open question**
> ([review-resolutions.md](review-resolutions.md) D9), not a minor detail — see the
> open questions below.

## The hierarchy

```text
org
 └─ team
     └─ project        long-running, team-owned; a Team Manager creates these
         └─ workflow    one batch of related work, instantiated from a template
             └─ task     the executable unit the scheduler runs
```

### Terminology (reconcile, don't overload)

[vision.md](../vision.md) deliberately avoided reusing "workflow"; the code drifted.
Pinned mapping so every downstream sentence is unambiguous:

| This doc | Code today | vision.md | Meaning |
| --- | --- | --- | --- |
| **Project** | *(new)* | *(new)* | long-running container; team-owned; holds many workflows |
| **Workflow** | `workflow` | Run | one batch instantiated from a recipe; owns tasks |
| (recipe) | `template` | Plan | reusable definition instantiated into a workflow |
| **Task** | `task` | Task | the executable unit |

**Project is the only new noun.** Workflow and task exist. Templates are
instantiated *as workflows inside a project*, so a manager spins up the Nth batch
of an ongoing project without re-authoring it.

## The distinction that makes it work: containment vs assignment

A project is **team-wide** (a manager sees all of it), but its workflows are **run
by individual members on their own edges**. Those are two different relationships,
and keeping them separate is exactly what keeps the sync model conflict-free
([sync-model.md](sync-model.md)):

- **Containment** — visibility & permissions. `task → workflow → project → team →
  org`. A manager traverses down to see everything in their team. This is the org
  chart; it is cloud-side graph only.
- **Assignment** — `scoped_to`, *who runs it / where it syncs*. A `workflow` (or
  `task`) `->scoped_to-> edge` — an **edge (daemon) identity, not a user**
  ([review-resolutions.md](review-resolutions.md) D2; a user owns N daemons). This
  is the routing edge the sync path reads ([tags-scope.md](../tags-scope.md)).

So: a project lives at the team; its workflows are assigned down to members; status
flows back up into the project view. **Project = ownership root; Workflow =
assignable unit; Task = executable.**

## The team graph (additions)

Same idiomatic `TYPE RELATION` shape as `depends_on` / `learned` / `tagged_with`:

```sql
DEFINE TABLE IF NOT EXISTS org     SCHEMALESS;
DEFINE TABLE IF NOT EXISTS team    SCHEMALESS;
DEFINE TABLE IF NOT EXISTS user    SCHEMALESS;
DEFINE TABLE IF NOT EXISTS project SCHEMALESS;
DEFINE FIELD IF NOT EXISTS status ON project TYPE string;     -- active | archived

DEFINE TABLE IF NOT EXISTS member_of TYPE RELATION SCHEMALESS; -- user→team, field: role
DEFINE TABLE IF NOT EXISTS under     TYPE RELATION SCHEMALESS; -- team→org, project→team
DEFINE TABLE IF NOT EXISTS edge      SCHEMALESS;               -- a registered daemon identity
DEFINE TABLE IF NOT EXISTS scoped_to TYPE RELATION SCHEMALESS; -- workflow|task→edge (assignment; D2)
```

- Containment lives on `under` (`project ->under-> team`, `workflow ->under->
  project`). Workflows still own their tasks as today.
- Assignment lives on `scoped_to` (single-cardinality; see [tags-scope.md](../tags-scope.md)).
- This graph is **cloud-only**. An edge knows its own identity and the workflows
  assigned to it, not the org chart.

## Roles

Three roles. Admin is global; manager/member is **per-team** (you can manage team A
and merely belong to team B), so those two live on the `member_of` edge while admin
is a global flag on the user.

| Role | Scope | Can |
| --- | --- | --- |
| **Admin** | org | everything — manage teams, users, roles, all projects, triggers, ingest |
| **Team Manager** | team(s) led | create/archive **projects**, instantiate & assign workflows, see all team status, team chat |
| **Member** | self | work assigned to them; create workflows within projects they're on; drive their own tasks |

Enforcement of these roles is [authz.md](authz.md) (SurrealDB PERMISSIONS for cloud
queries; Zenoh ACLs for the wire). **Locally, with no `[server]` config, there are
no roles** — the daemon trusts its single operator exactly as today.

## One UI, role-gated — not a second app

Reuse `ui/`. The same React app, with navigation and sections gated by the JWT's
role claim. **No second frontend.**

```text
Sidebar (role-gated):
  Projects            everyone — my projects → workflows → tasks
  Team    [manager+]  team dashboard, create project, assign workflow, team chat
  Admin   [admin]     users, teams, membership/roles, triggers, channels, ingest
```

The operator views that already exist — task detail, chat, hcom logs, the
workflow→tasks board — are **unchanged**; they simply nest *inside the project
drill-down* instead of sitting at the top level. The only genuinely new screens are
the Projects list, the Team dashboard, and the Admin area. Everything else is reuse
plus a `role` check on nav.

## Keyspace

The project becomes a segment in the Zenoh keyspace, so a whole project's streams
sit under one prefix ([sync-model.md](sync-model.md)):

```text
lazybones/{org}/{team}/{project}/workflow/{wf}/task/{id}/spec      # assignment (cloud→edge)
lazybones/{org}/{team}/{project}/workflow/{wf}/task/{id}/status    # facts (edge→cloud)
lazybones/{org}/{team}/{project}/channel/{kind}/{chanId}/msg/*     # project-scoped chat/feed
```

## What we are asking to approve

1. **Project** as a first-class noun between team and workflow, with the pinned
   terminology (Project / Workflow=Run / Template=Plan / Task).
2. **Containment (`under`) vs assignment (`scoped_to`)** as two distinct edges.
3. **Three roles** (Admin / Team Manager / Member) with admin global and
   manager/member on the `member_of` edge.
4. **One role-gated UI** reusing `ui/`, not a second frontend.

## Open questions — RESOLVED

All three are closed in [projects-decisions.md](projects-decisions.md) (phase-1
decision record); summarized here, see that file for the rationale and the schema.

- **Project targets one repo or many** → **many.** A project is an org/ownership
  noun; repos ride as project config or a `repo:*` tag, with no project→repo edge.
  Distinct from the existing repo/worktree concept.
- **Workflow co-run vs single edge** → **single edge.** `scoped_to` stays
  single-cardinality (D2); collaboration happens at the project/channel level, not by
  multi-homing a workflow. Moving work between edges is the D3 reassignment handoff.
- **Project-level roles** → **team-level for v1.** Admin is a global flag on `user`;
  manager/member live on `member_of.role`. A per-project lead can be added later
  without disturbing the team graph.
