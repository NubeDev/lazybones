//! Author-a-workflow round-trip over the in-process MCP router (design §6.1, task
//! `mcp-spike`).
//!
//! Drives the real [`McpServer::workflow_create`] tool the way the streamable-HTTP
//! transport does — a request's [`http::request::Parts`] (carrying the bearer token)
//! plus the typed args — against an in-memory store, with no HTTP-to-self. It proves
//! the P0 contract end to end:
//!
//! - a minted **`Author`** token creates a workflow that then exists via
//!   [`StoreHandle::get_run`] (the same store boundary the REST surface reads), and
//! - a **no-token** `workflow.create` is refused as `unauthorized` and writes
//!   nothing — authoring is gated by the capability exactly as REST is (§3).

use std::sync::Arc;

use rmcp::handler::server::tool::Extension;
use rmcp::handler::server::wrapper::Parameters;

use lazybones_auth::{ManagementProfile, ScopedSession};
use lazybones_mcp::McpServer;
use lazybones_mcp::SessionResolver;
use lazybones_mcp::args::{
    SkillArgs, TaskCreateArgs, TemplateArgs, WorkflowAddTaskArgs, WorkflowCreateArgs,
};
use lazybones_store::{StoreEngine, StoreHandle};

/// A one-token registry: the MCP twin of the API's token map, so the in-process
/// server authenticates a bearer token to its session exactly like a REST request.
struct OneToken {
    token: String,
    session: ScopedSession,
}

impl SessionResolver for OneToken {
    fn session_for(&self, token: &str) -> Option<ScopedSession> {
        (token == self.token).then(|| self.session.clone())
    }
}

/// A fresh in-memory store — the same `StoreEngine::Memory` the store's own tests use.
async fn store() -> StoreHandle {
    StoreHandle::open(&StoreEngine::Memory, "lazybones", "test", "test-secret-key")
        .await
        .expect("open in-memory store")
}

/// Build a server whose registry maps `token` to a freshly minted `Author` session
/// (the default management grant: `Read + Author + Document`).
fn server_with_author_token(store: StoreHandle, token: &str) -> McpServer {
    let session = ScopedSession::for_management("mcp-spike", ManagementProfile::Author);
    let resolver = Arc::new(OneToken {
        token: token.to_owned(),
        session,
    });
    McpServer::new(store, resolver)
}

/// An `Authorization: Bearer <token>` request's [`Parts`], as the streamable-HTTP
/// transport injects them into a tool call.
fn parts_with_token(token: &str) -> http::request::Parts {
    http::Request::builder()
        .header(http::header::AUTHORIZATION, format!("Bearer {token}"))
        .body(())
        .expect("build request")
        .into_parts()
        .0
}

/// A request's [`Parts`] with no `Authorization` header — the no-token path.
fn parts_without_token() -> http::request::Parts {
    http::Request::builder()
        .body(())
        .expect("build request")
        .into_parts()
        .0
}

fn create_args(id: &str) -> WorkflowCreateArgs {
    serde_json::from_value(serde_json::json!({
        "id": id,
        "title": "MCP spike workflow",
        "workspace": { "repo": "/home/user/code/rust/lazybones" },
    }))
    .expect("deserialize workflow.create args")
}

#[tokio::test]
async fn author_token_creates_workflow_that_exists_via_store() {
    let store = store().await;
    let token = "author-secret";
    let server = server_with_author_token(store.clone(), token);

    // The Author token creates the workflow over the tool surface.
    let result = server
        .workflow_create(
            Extension(parts_with_token(token)),
            Parameters(create_args("mcp-service")),
        )
        .await
        .expect("workflow.create should succeed for an Author token");

    // The tool returns the created run as JSON.
    assert_eq!(result.0["id"], "mcp-service");
    assert_eq!(result.0["lifecycle"], "active");

    // And it now exists via the *store* — the REST surface's read boundary. The two
    // front doors share one source of truth.
    let stored = store
        .get_run("mcp-service")
        .await
        .expect("store query")
        .expect("the created run exists");
    assert_eq!(stored.id, "mcp-service");
    assert_eq!(stored.title, "MCP spike workflow");
    assert_eq!(stored.workspace.repo, "/home/user/code/rust/lazybones");
}

#[tokio::test]
async fn no_token_workflow_create_is_refused() {
    let store = store().await;
    let server = server_with_author_token(store.clone(), "author-secret");

    // No `Authorization` header ⇒ no session ⇒ a mutator refuses (authoring is gated;
    // the unauthenticated path is read-only, design §3).
    // `Json<Value>` is not `Debug`, so unwrap the error via `.err()` rather than
    // `expect_err` (which would need to format the `Ok`).
    let err = server
        .workflow_create(
            Extension(parts_without_token()),
            Parameters(create_args("unauthorized-wf")),
        )
        .await
        .err()
        .expect("workflow.create must be refused without a token");
    assert_eq!(err.message, "unauthorized");

    // And nothing was written — a refused author is a no-op.
    assert!(
        store
            .get_run("unauthorized-wf")
            .await
            .expect("store query")
            .is_none(),
        "a refused workflow.create must not create a run"
    );
}

/// The full §6.1 author round-trip on a single `Author` token: create a workflow,
/// add a task to it (inline) and a task from a template, author a standalone task,
/// a template, and a skill — every authoring verb the default management token is
/// meant to drive — and assert each lands in the store. No lifecycle is touched, so
/// the workflow stays `active` with nothing promoted (authoring is not running).
#[tokio::test]
async fn author_token_round_trips_every_authoring_verb() {
    let store = store().await;
    let token = "author-secret";
    let server = server_with_author_token(store.clone(), token);
    let parts = || parts_with_token(token);

    // Workflow.
    server
        .workflow_create(Extension(parts()), Parameters(create_args("wf")))
        .await
        .expect("workflow.create");

    // Template, used to instantiate a workflow task below.
    let template: TemplateArgs = serde_json::from_value(serde_json::json!({
        "id": "tmpl",
        "title": "Reusable recipe",
        "spec_template": "do the thing",
        "default_tool": "claude",
    }))
    .expect("TemplateArgs");
    server
        .template_create(Extension(parts()), Parameters(template))
        .await
        .expect("template.create");
    assert!(
        store.get_template("tmpl").await.expect("query").is_some(),
        "template must exist after template.create"
    );

    // Inline workflow task.
    let inline: WorkflowAddTaskArgs = serde_json::from_value(serde_json::json!({
        "workflow_id": "wf",
        "id": "wf.inline",
        "title": "Inline task",
        "spec": "write the inline bit",
    }))
    .expect("WorkflowAddTaskArgs");
    server
        .workflow_add_task(Extension(parts()), Parameters(inline))
        .await
        .expect("workflow.add_task (inline)");

    // Workflow task instantiated from the template, depending on the inline one.
    let templated: WorkflowAddTaskArgs = serde_json::from_value(serde_json::json!({
        "workflow_id": "wf",
        "id": "wf.templated",
        "title": "Templated task",
        "from_template": "tmpl",
        "deps": ["wf.inline"],
    }))
    .expect("WorkflowAddTaskArgs");
    server
        .workflow_add_task(Extension(parts()), Parameters(templated))
        .await
        .expect("workflow.add_task (from_template)");

    // Both tasks are linked to the workflow via run_id, and the templated one took
    // the template's spec and template_id.
    let task_ids: Vec<String> = store
        .list_run_tasks("wf")
        .await
        .expect("list_run_tasks")
        .into_iter()
        .map(|t| t.id)
        .collect();
    assert!(task_ids.contains(&"wf.inline".to_owned()));
    assert!(task_ids.contains(&"wf.templated".to_owned()));
    let templated = store
        .get_task("wf.templated")
        .await
        .expect("query")
        .expect("templated task exists");
    assert_eq!(templated.spec, "do the thing");
    assert_eq!(templated.template_id.as_deref(), Some("tmpl"));
    assert_eq!(templated.deps, vec!["wf.inline".to_owned()]);

    // Standalone task (groups under the server's run label, not a workflow).
    let standalone: TaskCreateArgs = serde_json::from_value(serde_json::json!({
        "id": "solo",
        "title": "Standalone task",
        "spec": "do it alone",
    }))
    .expect("TaskCreateArgs");
    server
        .task_create(Extension(parts()), Parameters(standalone))
        .await
        .expect("task.create");
    assert!(
        store.get_task("solo").await.expect("query").is_some(),
        "standalone task must exist after task.create"
    );

    // Skill.
    let skill: SkillArgs = serde_json::from_value(serde_json::json!({
        "id": "review-rust",
        "title": "Review Rust",
        "body": "Check for unwrap() in non-test code.",
    }))
    .expect("SkillArgs");
    server
        .skill_create(Extension(parts()), Parameters(skill))
        .await
        .expect("skill.create");
    assert!(
        store.get_skill("review-rust").await.expect("query").is_some(),
        "skill must exist after skill.create"
    );

    // Authoring is not running: the workflow is still `active` and nothing was
    // promoted (every task remains `pending`).
    let run = store.get_run("wf").await.expect("query").expect("run exists");
    assert_eq!(run.lifecycle.as_str(), "active");
}
