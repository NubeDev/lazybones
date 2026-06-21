# Authz — team/user access on the team plane

> Status: spec, for approval. Backend-first.
> Audience: whoever builds cloud identity, permissions, and the Zenoh ACLs.
> Parent: [README.md](README.md). Roles & hierarchy: [projects.md](projects.md).

Access control is **data-driven**: it falls out of the team graph
(`member_of` role edges, `under` containment) rather than living in imperative
checks scattered through the API. SurrealDB is the authz engine *and* the JWT
issuer for the cloud plane. One thing to state up front, because it shapes
everything:

> **SurrealDB authz only gates SurrealDB queries. It does not gate the Zenoh wire.**
> So there is **one source of truth** (the `member_of` role edges) projected to
> **two enforcement points**: SurrealDB `PERMISSIONS` for cloud queries/UI, and
> Zenoh ACLs for what an edge may pub/sub.

```text
            member_of role edges   ← single source of truth (cloud graph)
               /            \
   SurrealDB PERMISSIONS    Zenoh ACLs
   (cloud queries + the      (the wire: what an edge may
    manager/admin UI)         publish/subscribe; minted at enrollment)
```

## Identity: SurrealDB RECORD access (also the JWT issuer)

App users (managers, members) authenticate against the `user` table via a RECORD
access; SurrealDB issues a **JWT** and sets `$auth` to the authenticated user
record. This *is* the cloud's identity issuer — no separate auth service for
phase 1. The same JWT is what an edge presents on its Zenoh session.

```surql
DEFINE ACCESS user ON DATABASE TYPE RECORD
  SIGNIN ( SELECT * FROM user
           WHERE email = $email AND crypto::argon2::compare(pass, $pass) )
  DURATION FOR TOKEN 15m, FOR SESSION 12h;
```

External OIDC/Keycloak is a later seam: swap RECORD access for a `TYPE JWT` access
that trusts the external issuer; the permission clauses below are unchanged.

## The model: where roles live

- **Admin** is **global** — a flag on the `user` record (`admin = true`). Org-wide.
- **Team Manager / Member** are **per-team** — `role` on the `member_of` edge — so
  one user can manage team A and merely belong to team B.

```text
user ->member_of-> team   (role: 'manager' | 'member')
user.admin = true          (global admin)
project ->under-> team     (containment; drives visibility)
```

## Permissions: visibility falls out of the graph

Every query is automatically scoped to `$auth` by the DB; the API writes no
visibility checks. Representative clause (exact traversal syntax TBD at
implementation):

```surql
-- a project is visible to admins and to anyone in its team;
-- writable only by admins and managers of its team
DEFINE TABLE project SCHEMALESS PERMISSIONS
  FOR select WHERE $auth.admin = true
             OR ->under->team IN (SELECT ->member_of->team FROM $auth)
  FOR create, update, delete WHERE $auth.admin = true
             OR ->under->team IN (SELECT ->member_of[WHERE role='manager']->team FROM $auth);
```

Workflows and tasks inherit the same rule one hop down through their project.
**Change a role once on the `member_of` edge → every query re-evaluates live.** Keep
permission expressions shallow (one or two hops) and unit-test them — deep
traversals in a `PERMISSIONS` clause are the part that bites on performance.

## Enforcement nuance: embedded engine + sessions

The cloud SurrealDB is **embedded** (`engine::local::Db`). `PERMISSIONS` only apply
to a session **authenticated as a user**; opened as root, they are bypassed. Two
options, identical model, different locus:

- **A — authenticated sessions.** `db.authenticate(<user JWT>)` so the DB enforces
  `PERMISSIONS` declaratively. **Caveat (review):** doing this per-request on a
  *shared* `Surreal<Db>` handle **races** — one request's auth state bleeds into
  another's. A needs a **connection-per-request pool** first.
- **B — trusted server + API guard (default).** Server opens root; the existing
  [guard.rs](../../crates/lazybones-api/src/routes/guard.rs) seam enforces the *same*
  clauses imperatively against the team graph. No shared-handle race.

**Default to B** ([review-resolutions.md](review-resolutions.md) D5); reach for A
only with a real connection pool. Note: in the phase-1 **remote SurrealDB** model
the cloud already uses pooled, per-connection authenticated sessions, so `PERMISSIONS`
enforce directly there — that is the natural home for A. The role model
(`member_of`, `under`) is identical either way.

## The wire: Zenoh ACLs (the half SurrealDB can't see)

Cloud↔edge data crosses Zenoh, not a SurrealDB query, so it needs its own gate —
minted from the same `member_of` edges at enrollment and re-minted when a role
changes:

| Principal | Zenoh ACL (key-expr scope) |
| --- | --- |
| **Member (edge)** | pub/sub only `lazybones/{org}/{team}/{project}/.../{thisUser}/**` |
| **Manager (dashboard)** | additional **subscribe** on `lazybones/{org}/{team}/**` |
| **Admin** | subscribe on `lazybones/{org}/**` |

mTLS provides the session identity; the JWT (from RECORD access) carries the
claims. **The two enforcement points are not atomic** ([review-resolutions.md](review-resolutions.md)
D6): SurrealDB `PERMISSIONS` re-evaluate live (a demotion cuts access on the next
query), but an already-issued JWT/cert keeps wire access until it expires or the
edge reconnects. So tokens need **short TTL + a refresh/revocation check**; do not
claim instantaneous revocation. (This whole Zenoh-ACL layer is **deferred** with the
fabric — D1 — and only returns if offline/P2P edges do; in the phase-1 remote model
there is a single enforcement point and the gap shrinks to "next query".)

## Edge-local: no record-auth

The **edge** SurrealDB stays single-operator and trusting
([edge-changes.md](edge-changes.md)). Identity and authz matter only at the
**network boundary** — what this daemon may pub/sub — never for its local REST.
Don't run RECORD access on the edge.

## What we are asking to approve

1. **SurrealDB RECORD access** as the phase-1 identity issuer + JWT, with an OIDC
   `TYPE JWT` seam for later.
2. **Data-driven `PERMISSIONS`** keyed on `$auth` + the team graph as the cloud
   enforcement, with admin global and manager/member on `member_of`.
3. **Two enforcement points, one source of truth** — SurrealDB permissions *and*
   Zenoh ACLs both minted from `member_of`.
4. Authenticated-session enforcement (**option A**) as the default.

## Open questions

- **A vs B** — authenticated embedded sessions vs API-layer guard. Recommend A;
  validate that the embedded engine enforces `PERMISSIONS` cleanly under
  `db.authenticate`.
- **Namespace-per-org?** One SurrealDB namespace per `org` gives hard multi-tenant
  isolation (separate-company SaaS) vs row-level permissions in one DB (single
  company, many teams). Recommend one DB + permissions for now.
- **Token lifetime / refresh** — 15m token in the example; confirm refresh flow for
  long-lived edge sessions vs short-lived UI sessions.
