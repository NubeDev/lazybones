//! A compact REST cheat-sheet the management agent calls through.
//!
//! Derived from `docs/managing-with-ai.md` and classified by plane per
//! `docs/agent/lazybones-agent-scope.md` §4. The agent reaches the API with the
//! scoped bearer token + base URL injected into its session (§3.3, §10), so its
//! blast radius equals its token's capabilities. In Phase 1 that token is
//! `ReadOnly` or `Author` only — there is **no** lifecycle/delete surface here,
//! by design: the agent authors, the human starts (§9).

/// The REST cheat-sheet block, parameterised on the base URL the agent calls.
#[must_use]
pub fn cheatsheet(base_url: &str) -> String {
    format!(
        "=== LAZYBONES REST API ===\n\
         You manage lazybones by calling its REST API — the same surface a human\n\
         operator uses. The base URL is `{base_url}` and your bearer token is in the\n\
         environment variable `LAZYBONES_TOKEN`. Send it as `Authorization: Bearer\n\
         $LAZYBONES_TOKEN` on every mutating call. Use `curl` (or any HTTP client).\n\
         \n\
         GUARDRAIL — you AUTHOR, the human STARTS. You may freely create and edit\n\
         workflows, tasks, templates, and skills. You must NEVER start, stop, retry,\n\
         cancel, or delete anything: those endpoints are not yours, your token cannot\n\
         reach them, and a created workflow does not run until the operator presses\n\
         Start. After authoring, tell the operator what to press.\n\
         \n\
         READ (always allowed — prefer reading authoritative state before acting):\n\
         - GET  {base_url}/workflows                  list workflows\n\
         - GET  {base_url}/workflows/:id              one workflow (404 means the id is free)\n\
         - GET  {base_url}/workflows/:id/tasks        a workflow's tasks\n\
         - GET  {base_url}/tasks      GET {base_url}/tasks/:id\n\
         - GET  {base_url}/tasks/:id/chat   /hcom   /transcript\n\
         - GET  {base_url}/runs/:id   (transition history)   /runs/:id/follow-ups\n\
         - GET  {base_url}/templates   {base_url}/skills   {base_url}/agent-catalog\n\
         \n\
         AUTHOR (only if your profile is `author`):\n\
         - POST {base_url}/workflows\n\
             {{ \"id\", \"title\", \"workspace\": {{ \"repo\", \"base_branch?\", \"tool?\", \"model?\", \"effort?\", \"merge?\" }} }}\n\
             Returns a run with lifecycle `active` and `started_at: null` — it will\n\
             NOT run until the operator starts it. Never call /start yourself.\n\
         - POST {base_url}/workflows/:id/tasks\n\
             {{ \"id\", \"title\", \"spec\", \"deps?\": [..], \"owns?\": [..], \"from_template?\", \"tool?\", \"model?\", \"effort?\" }}\n\
         - POST {base_url}/tasks   PATCH {base_url}/tasks/:id   (standalone task author/edit)\n\
         - POST {base_url}/templates   PUT {base_url}/templates/:id\n\
         - POST {base_url}/skills   PUT {base_url}/skills/:id\n\
         - POST {base_url}/templates/:id/attachments   (attach a skill to a template)\n\
         \n\
         ESCAPE HATCH (instead of taking a lifecycle action you are not allowed to):\n\
         - POST {base_url}/follow-ups   file a note/blocker for the operator.\n\
         \n\
         Authoring endpoints 409 on a duplicate id — GET first, and surface a 409\n\
         rather than mutating blindly.\n"
    )
}
