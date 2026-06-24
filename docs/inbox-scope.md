# Inbox & Outbox — one thread per person, across every platform

> Status: spec, ready to build. Backend-first; the UI is the last phase but
> "easy to use" is a first-class requirement, not an afterthought.
> Audience: whoever implements the contact graph, the channel layer, and the feed.
> Read [SCOPE.md](../SCOPE.md) for the principles and
> [hcom-logs-scope.md](hcom-logs-scope.md) for the live SSE model this builds on.
>
> The concept is borrowed (clean-room) from Macro's unified inbox — *you should
> only have to check one thing* — but the unit here is a **person**, not a
> platform. We owe Macro the idea; none of its code (it is AGPLv3; we read the
> product docs, not the source).

## The user story this exists to serve

> 1. I talk to **Tom** in three places — Slack, GitHub, and email. Today that is
>    three apps and three notification streams for *one relationship*.
> 2. I want **one thread for Tom** that merges his Slack DMs, his GitHub mentions
>    and PR comments, and his emails — newest first, regardless of where each
>    message landed.
> 3. When I reply, I **pick the channel each time** (defaulting to wherever his
>    last message came from), and the outbox sends it out on that platform as me.
> 4. Linking is **mine to control**: I explicitly say "these three handles are
>    Tom." The system can *suggest* a merge, but never silently fuses two people.
> 5. And — because this is lazybones — my **agent fleet is just another sender**.
>    A blocked task or an agent's consent question shows up in the same feed as a
>    person would, and I can answer it the same way.

Points 1–4 are the unified comms hub. Point 5 is the synthesis that keeps this a
lazybones feature rather than a separate product (see *Synthesis*, below).

## The core is identity, not messaging

Sending and receiving on each platform is mechanical, well-trodden Rust. The
feature *is* the **contact graph** — resolving `slack:U07ABC`, `github:tomhandle`,
and `email:tom@acme.com` to one **Contact "Tom"**. Get that right and the inbox is
a group-by; get it wrong and you have three apps in one window.

```
Contact "Tom"
  ├─ Identity  slack:U07ABC        (linked by me)
  ├─ Identity  github:tomhandle    (suggested → I confirmed)
  └─ Identity  email:tom@acme.com  (linked by me)

inbound Message ──resolve via Identity──▶ Contact ──▶ Thread (one per Contact)
outbound Draft  ──I pick a channel────────────────▶ Send on that platform
```

## Data model

Four durable records (SurrealDB rows, modelled on the existing `event` /
`follow_up` tables — auto-minted ULID keys, wire projections that leak no DB types):

- **`contact`** — a person/relationship. `id`, `display_name`, optional avatar, free
  notes. Holds no platform specifics.
- **`identity`** — one platform handle, edged to a contact:
  `identity ->belongs_to-> contact`. `{ platform: slack|github|email, handle,
  contact: Option<…>, linked_by: me|suggested|unlinked, confidence }`. An
  **unlinked** identity (no contact yet) is a first-class state — see below.
- **`message`** — a normalized inbound or outbound message. `{ id, identity,
  contact?, platform, direction: in|out, body (markdown), platform_ref (native
  id, for reply-threading), ts, status: unread|done }`. The native platform keeps
  the source of truth; this is a normalized projection, not a mirror.
- **`draft`/outbound** — an outgoing message: `{ contact, chosen_platform, body,
  in_reply_to?, ts, send_state }`.

A **Thread** is not stored — it is `messages` grouped by `contact` (or by unlinked
`identity`), newest first. The inbox is a projection, like
[`follow_up`](../crates/lazybones-store/src/follow_up) is.

## Identity merge — manual-first, with auto-suggest (decided)

The headline feature, built to never surprise:

- **Default for any new sender is `unlinked`.** A message from a handle we have
  never seen creates an `identity` with `contact = None`; it appears in the inbox
  under its raw handle. Nothing is merged automatically.
- **I link explicitly.** A `POST /contacts/:id/link { identity }` (or "new contact
  from this identity") attaches a handle to a Contact. `linked_by = me`.
- **The system only *suggests*.** A background matcher proposes merges on cheap,
  legible signals — shared verified email, identical display name, a GitHub
  profile email matching an email identity — and files them as `suggested` links
  the inbox surfaces as *"Looks like Tom — confirm?"*. A suggestion is inert until
  I accept it; rejecting it records a negative so it is not re-proposed.
- **Unmerge is always available** and records a negative match, so a bad link (or
  bad suggestion) is one action to undo and never recurs.

This keeps the trust model simple: the graph only ever fuses people on my say-so;
automation does discovery, not commitment.

## Inbox (read model)

`GET /inbox` returns threads — one per Contact, plus one per still-`unlinked`
identity — sorted by most-recent message. Each thread carries the merged message
list across all that contact's identities. Filters: `?contact=`, `?platform=`,
`?status=unread`, and `?suggested=1` to triage pending merge suggestions.

Marking done is inbox-zero: `POST /inbox/:thread/done` flips the thread's open
messages to `done`. Live updates ride the existing
[`LiveEvent`](../crates/lazybones-store/src/event/bus.rs) bus + `/stream` SSE — a
new inbound message publishes, the UI re-derives, no polling.

## Outbox — pick the channel each time (decided)

`POST /outbox { contact, body, in_reply_to? }` **requires a `platform`**, and the
composer **defaults it to the platform of the message being replied to** (or the
contact's most-recent inbound). I can override per message — reply to Tom's email
*on Slack* if I want. No stored per-contact preferred channel in v1; the default
is computed, the choice is always mine. The chosen `Channel` impl sends it, the
outbound `message` is recorded `direction = out`, and it threads into the same
Contact view.

## The `Channel` trait — per-platform, mature libs

Each platform implements one trait for send **and** receive; the matcher and inbox
never know which platform a message came from:

```rust
#[async_trait]
trait Channel {
    fn platform(&self) -> Platform;
    async fn send(&self, to: &Handle, msg: &OutMessage) -> Result<PlatformRef>;
    fn receive(&self) -> impl Stream<Item = InMessage>;   // long-lived, daemon-driven
}
```

v1 impls — three solid per-platform crates, **not** an 11-platform aggregator:

- **Slack** — `slack-morphism` (Socket Mode → receive, Web API → send). Mature.
- **GitHub** — `octocrab` for issue/PR-comment + mention ingestion; and we already
  have [`lazybones-gh`](../crates/lazybones-gh) wrapping the `gh` CLI, so half the
  plumbing exists. ("GitHub messages" = PR/issue comments and `@`-mentions.)
- **Email** — `async-imap` (receive) + `lettre` (send). Mature.

Wrapping everything behind `Channel` means a future swap to a unified crate
([`chat-system`](https://github.com/rexlunae/chat-system)) or a notification-only
crate ([`pling`](https://lib.rs/crates/pling), egress-only) for extra platforms is
an additive impl, not a rewrite. See the survey at the end for why we are *not*
starting with those.

## Synthesis — the agent fleet is just another `Channel`

This is what keeps the feature native to lazybones instead of a bolted-on email
client. The existing operator-attention surfaces become one more **system
Contact** (or per-agent contacts):

- A [`follow_up`](../crates/lazybones-store/src/follow_up) (consent / credential /
  gate / blocked), a finished/PR-ready task, and a management-agent question are
  each ingested as inbound `message`s from a `lazybones` / agent identity.
- Replying to an agent question routes through the agent `Channel` (the
  hcom/management-agent path), exactly as replying to Tom routes through Slack.

So one keyboard-driven feed holds *"Tom emailed you about the API"* next to
*"agent on task `auth` is stuck on a consent screen"* — and Signal/Noise (from the
earlier draft) survives as a derived classifier over message `kind`.

## Easy to use — a requirement, not a phase

The whole point is *one thing to check*, so the UI (`ui/src/features/inbox`) is
keyboard-first, mirroring the documented muscle memory: `g i` to open, `j/k` to
move, `space` to preview, `enter` for fullscreen, `e` to mark done, `shift+↓` to
multi-select, and a compose box where the channel dropdown is pre-filled with the
reply default. Contact-merge suggestions surface inline as a one-key confirm.

## REST surface

| Method · path | Job |
| --- | --- |
| `GET /inbox` (`?contact=&platform=&status=&suggested=`) | threads, newest-first |
| `GET /inbox/:thread` | one contact's merged message history |
| `POST /inbox/:thread/done` | inbox-zero a thread (+ batch variant) |
| `GET /contacts` / `POST /contacts` | list / create a Contact |
| `POST /contacts/:id/link` / `…/unlink` | attach / detach an identity (records negatives) |
| `GET /contacts/suggestions` | pending auto-suggested merges |
| `POST /outbox` (`{contact, platform, body, in_reply_to?}`) | send on the chosen platform |

## Architecture note

This is a self-contained capability with no dependency on build orchestration, so
it lands as its **own crate** (`lazybones-inbox`) in the workspace — store rows +
the `Channel` trait + the matcher — with the agent fleet wired in as one `Channel`
impl. It reuses the live bus, the auth/session model, and the encrypted secret
store (for per-platform OAuth tokens) verbatim.

## Phasing (contact graph → channels → inbox → outbox → UI)

1. **Contact graph** — `contact`/`identity`/`message` rows, link/unlink with
   negatives, and the inbox projection. Pure, unit-testable; ingest via a stub
   `Channel` (Console) so the whole model is exercised with no real platform.
2. **Channels (ingress)** — Slack, GitHub, email `receive()` impls feeding
   normalized `message`s; daemon keeps them online. `curl /inbox` now shows real
   merged threads.
3. **Suggest** — the background matcher proposing `suggested` links.
4. **Outbox** — `POST /outbox` + per-platform `send()`, channel chosen per message.
5. **UI** — the keyboard-first inbox/outbox feature.

## Out of scope (v1)

- A message **store of record** — platforms keep the truth; we hold a normalized
  projection and re-fetch on demand.
- **Auto-merging** identities without my confirmation (only ever suggested).
- A stored per-contact **preferred channel** — the send channel is chosen each time.
- Reactions / avatars / threading richness beyond reply-to.
- The 11-platform aggregator (`chat-system`) — three mature per-platform libs cover
  Slack+GitHub+email at far lower churn risk; revisit only when a fourth+ platform
  is actually wanted, then add it as a `Channel` impl.
