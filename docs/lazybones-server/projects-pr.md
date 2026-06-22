# Projects — team-graph foundation (PR overview)

> Status: delivered. This is the change record for the `lazy/projects` branch
> (base `master`) — the phase-1 **team graph** landing as code. It maps each
> commit to the spec it builds against, so a reviewer can read the PR top-down.
> Specs: [README.md](README.md) (the two-plane architecture),
> [projects.md](projects.md) (org hierarchy + roles),
> [projects-decisions.md](projects-decisions.md) (the ratified phase-1 contract),
> [authz.md](authz.md) (the access-control model).

## One sentence

This branch turns the **phase-1 / phase-3 team graph** from spec into running
code: the `org → team → project` hierarchy and its `member_of` / `under`
relations in the store, the REST surface that creates and lists them under
role-gated authz, and the role-aware Projects / Team / Admin UI on top — built
strictly to the ratified decisions in
[projects-decisions.md](projects-decisions.md), no new architecture invented.

## What ships (by commit)

The branch is four task commits; each is a self-contained, buildable layer.

| Commit | Task | Layer | Builds against |
| --- | --- | --- | --- |
| `2f008fa` | 1 | **Design ratified** — closes the three phase-1 open questions, pins the exact SurrealDB table/relation defs, fixes the D4 id-namespacing rule | [projects-decisions.md](projects-decisions.md) |
| `fbc051b` | 2 | **Store layer** — `org` / `team` / `user` / `project` rows + `member_of` / `under` relations in `crates/lazybones-store/src/{team,user}/` | §2 schema, [README.md](README.md) "the team graph" |
| `517698c` | 3 | **REST + authz** — project/team routes, role-gated by the `member_of` role edge | [authz.md](authz.md), [projects.md](projects.md) |
| `c236644` | 4 | **Role-gated UI** — Projects, Team dashboard, Admin sections reusing `ui/`, gated by `use-role` | [projects.md](projects.md) "one role-gated UI" |

Together these implement the phase-3 line of [README.md](README.md)'s phasing
("Projects + teams + authz + UI") on the schema contract frozen in task 1.

## How it holds to the contract

The decisions that shaped the schema (and are visible in this code):

- **Containment vs assignment stay distinct.** `under` (org chart →
  visibility/authz) and `scoped_to` (the single sync-routing edge) are separate
  relations — the rule that keeps sync conflict-free
  ([README.md](README.md) "two distinct relationships").
- **Team-level roles only for v1.** `member_of.role ∈ {manager, member}` plus a
  global `admin` bool on `user`; no per-project lead (Q3). The UI's three
  sections gate off exactly these.
- **`project` is a containment/authz anchor, not a repo binding.** No `repo`
  field; targets live in config / `repo:*` tags (Q1). The store keeps `project`
  SCHEMALESS with only `status` + denormalized `team` declared.
- **Org-graph rows are cloud-authored, single-writer**, so they keep plain ids —
  the `{org}/{edge}` namespacing (§3) applies only to syncable run facts, which
  this foundation does not touch yet.

## Scope / not in scope

- **In:** the cloud-only team graph (store + REST + UI) and its role gating.
- **Out (later phases):** the edge↔cloud sync boundary (outbox / LIVE-SELECT),
  channels/triggers/feed, enrollment + JWT/mTLS issuance. Those gate on the
  phase-1 boundary work, not on this graph
  ([README.md](README.md) "Phasing", [projects-decisions.md](projects-decisions.md) closing notes).

## Reviewing

- Start at [projects-decisions.md](projects-decisions.md) §2 — the schema in
  task 1 is the contract the other three commits build to.
- Store layer mirrors the existing edge `src/api/` grain (verb-per-file,
  SCHEMALESS rows, Rust row types own the full shape).
- UI adds sections to the existing `ui/`, not a second app; role gating is
  centralized in `ui/src/lib/hooks/use-role.ts`.
