# task: auth — scoped session + capability grants

## Goal
A small, honest authorization boundary. The trusted loop holds every capability;
an agent session is scoped to its single task and only the capabilities it needs
to drive that task and write memory.

## Deliverables
- `Capability` enum (Sync, Claim, Heartbeat, Done, Block, Memorize, Read).
- `ScopedSession::for_loop` (all caps, no task binding) and `for_agent` (task-bound
  subset, no `Sync`).
- `can(cap)` and `may_act_on(task_id)` checks; `AuthError` for refusals.

## Done definition
- The loop can drive any task; an agent token can only act on the task it was
  minted for (proved by the API test `agent_cannot_act_on_another_task`).
- A request with no/unknown bearer token is rejected before any handler body runs.
