//! The team-graph REST surface: projects, teams, and membership, plus the
//! role-gated authz guard.
//!
//! Like `documents_test.rs` this needs no engine or git — a store + router is
//! enough. Two worlds are exercised: the **local single-operator** daemon (no
//! `[server]` config ⇒ no roles ⇒ the guard passes through), and a
//! **roles-enabled** server where the guard reads the principal's org-graph role
//! (admin / team-manager / member) off the team graph and gates verbs per the
//! projects.md Roles table.

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use lazybones_api::{AppState, router};
use lazybones_auth::{ManagementProfile, ScopedSession};
use lazybones_store::{FileBlobStore, StoreEngine, StoreHandle, Team, User};
use serde_json::{Value, json};
use tower::ServiceExt;

const LOOP_TOKEN: &str = "loop-tok";

/// Build state (optionally roles-enabled) plus its store, so a test can seed the
/// team graph through the store before driving the router.
async fn state(roles: bool) -> AppState {
    let store = StoreHandle::open(&StoreEngine::Memory, "lazybones", "test", "key")
        .await
        .unwrap();
    let dir = std::env::temp_dir().join(format!(
        "lazybones-api-proj-{}",
        lazybones_store::sha256_hex(format!("{:p}", &store).as_bytes())
    ));
    AppState::new(store, "run", "http://127.0.0.1:0", LOOP_TOKEN)
        .with_assets(Arc::new(FileBlobStore::new(dir)))
        .with_roles(roles)
}

/// The local single-operator world: no roles, the loop token is the operator.
async fn local_app() -> Router {
    router(state(false).await)
}

async fn send(app: &Router, req: Request<Body>) -> (StatusCode, Value) {
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let body = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (status, body)
}

fn json_req(method: &str, path: &str, token: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(path)
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

fn loop_json(method: &str, path: &str, body: Value) -> Request<Body> {
    json_req(method, path, LOOP_TOKEN, body)
}

fn get(path: &str) -> Request<Body> {
    Request::builder()
        .method("GET")
        .uri(path)
        .body(Body::empty())
        .unwrap()
}

/// Local mode (no `[server]` config): the operator drives the whole team graph
/// with the loop token and the role guard never bites — the passthrough case.
#[tokio::test]
async fn local_no_roles_full_crud_passthrough() {
    let app = local_app().await;

    // A mutation still needs *a* token (the capability layer is untouched).
    let (s, _) = send(
        &app,
        Request::builder()
            .method("POST")
            .uri("/teams")
            .header("content-type", "application/json")
            .body(Body::from(json!({"id":"platform","title":"Platform"}).to_string()))
            .unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED, "team create needs a token");

    // Team create (idempotent), then a project under it.
    let (s, body) = send(
        &app,
        loop_json("POST", "/teams", json!({"id":"platform","title":"Platform"})),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "create team: {body}");

    let (s, body) = send(
        &app,
        loop_json(
            "POST",
            "/projects",
            json!({"id":"apollo","title":"Apollo","team":"platform","repos":["repo:app"]}),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "create project: {body}");
    assert_eq!(body["status"], "active");
    assert_eq!(body["team"], "platform");
    assert_eq!(body["repos"][0], "repo:app");

    // Duplicate project id → 409.
    let (s, _) = send(
        &app,
        loop_json("POST", "/projects", json!({"id":"apollo","title":"dupe"})),
    )
    .await;
    assert_eq!(s, StatusCode::CONFLICT);

    // Open reads: get + list (+ the team filter), and the team traversal.
    let (s, body) = send(&app, get("/projects/apollo")).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body["title"], "Apollo");
    let (s, body) = send(&app, get("/projects?team=platform")).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body.as_array().unwrap().len(), 1);
    let (s, body) = send(&app, get("/teams/platform/projects")).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body[0]["id"], "apollo", "under traversal finds the project");

    // Update preserves status + team; archive flips status, keeps the row.
    let (s, body) = send(
        &app,
        loop_json("PUT", "/projects/apollo", json!({"title":"Apollo v2","repos":[]})),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body["title"], "Apollo v2");
    assert_eq!(body["status"], "active", "update does not archive");
    assert_eq!(body["team"], "platform", "owning team preserved");

    let (s, body) = send(&app, loop_json("POST", "/projects/apollo/archive", Value::Null)).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body["status"], "archived");
    // Archived rows survive (no hard delete).
    let (s, body) = send(&app, get("/projects/apollo")).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body["status"], "archived");

    // Membership add/remove with a role.
    let (s, body) = send(
        &app,
        loop_json("POST", "/teams/platform/members", json!({"user":"ada","role":"manager"})),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "add member: {body}");
    assert_eq!(body["role"], "manager");
    let (s, body) = send(&app, get("/teams/platform/members")).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body.as_array().unwrap().len(), 1);
    let (s, body) = send(
        &app,
        loop_json("DELETE", "/teams/platform/members/ada", Value::Null),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body["removed"], true);

    // A missing project → 404.
    let (s, _) = send(&app, get("/projects/ghost")).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}

/// Roles-enabled world: an admin and a manager clear their verbs (happy path); an
/// under-privileged member is refused `403`.
#[tokio::test]
async fn roles_gate_verbs_by_principal() {
    let st = state(true).await;

    // Seed the graph: an admin, a manager of `platform`, and a plain member.
    let now = st.store.now();
    st.store
        .create_user(&User::new("root", "Root", &now).as_admin())
        .await
        .unwrap();
    st.store.create_user(&User::new("mgr", "Manager", &now)).await.unwrap();
    st.store.create_user(&User::new("mem", "Member", &now)).await.unwrap();
    st.store
        .create_team(&Team::new("platform", "Platform", &now))
        .await
        .unwrap();
    st.store
        .add_member("mgr", "platform", lazybones_store::MemberRole::Manager)
        .await
        .unwrap();
    st.store
        .add_member("mem", "platform", lazybones_store::MemberRole::Member)
        .await
        .unwrap();

    // Register one token per principal — actor == user id, with an authoring grant.
    let mint = |actor: &str| {
        let tok = format!("tok-{actor}");
        st.register_agent(
            tok.clone(),
            ScopedSession::for_management(actor.to_owned(), ManagementProfile::Author),
        );
        tok
    };
    let admin_tok = mint("root");
    let mgr_tok = mint("mgr");
    let mem_tok = mint("mem");
    let app = router(st);

    // Happy path: a manager of `platform` may create a project there.
    let (s, body) = send(
        &app,
        json_req(
            "POST",
            "/projects",
            &mgr_tok,
            json!({"id":"apollo","title":"Apollo","team":"platform"}),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "manager creates in their team: {body}");

    // 403: a mere member may not create a project (under-privileged role).
    let (s, _) = send(
        &app,
        json_req(
            "POST",
            "/projects",
            &mem_tok,
            json!({"id":"gemini","title":"Gemini","team":"platform"}),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "member cannot create a project");

    // 403: a manager is not an admin — creating a team is admin-only.
    let (s, _) = send(
        &app,
        json_req("POST", "/teams", &mgr_tok, json!({"id":"growth","title":"Growth"})),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "team create is admin-only");

    // Admin clears everything: create the team, then add a member.
    let (s, _) = send(
        &app,
        json_req("POST", "/teams", &admin_tok, json!({"id":"growth","title":"Growth"})),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "admin creates a team");
    let (s, _) = send(
        &app,
        json_req(
            "POST",
            "/teams/platform/members",
            &admin_tok,
            json!({"user":"newbie","role":"member"}),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "admin manages membership");

    // 403: a member may not manage membership either.
    let (s, _) = send(
        &app,
        json_req(
            "POST",
            "/teams/platform/members",
            &mem_tok,
            json!({"user":"x","role":"member"}),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "member cannot manage membership");

    // Reads stay open even in roles mode (no token needed).
    let (s, body) = send(&app, get("/projects?team=platform")).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body.as_array().unwrap().len(), 1, "only the manager's project landed");
}
