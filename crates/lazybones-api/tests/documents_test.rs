//! The document-writer REST surface: documents, references, assets, branding,
//! sources, render/export, and the `Capability::Document` guard.
//!
//! Like `skills_test.rs` this needs no engine or git — a store + router (with a
//! tempdir blob store) is enough. The GitHub publishing routes (`gh/*`,
//! `publish`) are exercised separately against a real repo; here we cover
//! everything reachable without shelling out.

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use lazybones_api::{AppState, router};
use lazybones_store::{FileBlobStore, StoreEngine, StoreHandle};
use serde_json::{Value, json};
use tower::ServiceExt;

const LOOP_TOKEN: &str = "loop-tok";

async fn app() -> Router {
    let store = StoreHandle::open(&StoreEngine::Memory, "lazybones", "test", "key")
        .await
        .unwrap();
    let dir = std::env::temp_dir().join(format!(
        "lazybones-api-blob-{}",
        lazybones_store::sha256_hex(format!("{:p}", &store).as_bytes())
    ));
    let state = AppState::new(store, "run", "http://127.0.0.1:0", LOOP_TOKEN)
        .with_assets(Arc::new(FileBlobStore::new(dir)));
    router(state)
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

/// Raw send returning the content-type header and body bytes (for assets/PDF).
async fn send_raw(app: &Router, req: Request<Body>) -> (StatusCode, String, Vec<u8>) {
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let ct = res
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_owned();
    let bytes = res.into_body().collect().await.unwrap().to_bytes().to_vec();
    (status, ct, bytes)
}

fn loop_json(method: &str, path: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(path)
        .header("authorization", format!("Bearer {LOOP_TOKEN}"))
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

fn get(path: &str) -> Request<Body> {
    Request::builder()
        .method("GET")
        .uri(path)
        .body(Body::empty())
        .unwrap()
}

#[tokio::test]
async fn branding_crud_and_guard() {
    let app = app().await;

    // Create without a token → 401 (mutations are guarded).
    let anon = Request::builder()
        .method("POST")
        .uri("/branding")
        .header("content-type", "application/json")
        .body(Body::from(json!({"id":"acme","name":"Acme"}).to_string()))
        .unwrap();
    let (s, _) = send(&app, anon).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED, "branding create needs a token");

    // Create with the loop token.
    let (s, body) = send(
        &app,
        loop_json(
            "POST",
            "/branding",
            json!({"id":"acme","name":"Acme","colors":{"primary":"#ff0000"}}),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "create branding: {body}");
    assert_eq!(body["colors"]["primary"], "#ff0000");

    // Duplicate id → 409.
    let (s, _) = send(
        &app,
        loop_json("POST", "/branding", json!({"id":"acme","name":"dupe"})),
    )
    .await;
    assert_eq!(s, StatusCode::CONFLICT);

    // Open read: get + list.
    let (s, body) = send(&app, get("/branding/acme")).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body["name"], "Acme");
    let (s, body) = send(&app, get("/branding")).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body.as_array().unwrap().len(), 1);

    // Update + delete.
    let (s, body) = send(
        &app,
        loop_json("PUT", "/branding/acme", json!({"name":"Acme v2"})),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body["name"], "Acme v2");
    let (s, body) = send(&app, loop_json("DELETE", "/branding/acme", Value::Null)).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body["deleted"], true);
}

#[tokio::test]
async fn asset_upload_serve_dedup_delete() {
    let app = app().await;
    let png = b"\x89PNG\r\n\x1a\nfake-logo-bytes".to_vec();

    let upload = |bytes: Vec<u8>, name: &str| {
        Request::builder()
            .method("POST")
            .uri("/assets")
            .header("authorization", format!("Bearer {LOOP_TOKEN}"))
            .header("content-type", "image/png")
            .header("x-filename", name)
            .body(Body::from(bytes))
            .unwrap()
    };

    // Upload needs a token.
    let anon = Request::builder()
        .method("POST")
        .uri("/assets")
        .header("content-type", "image/png")
        .body(Body::from(png.clone()))
        .unwrap();
    let (s, _) = send(&app, anon).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);

    let (s, body) = send(&app, upload(png.clone(), "logo.png")).await;
    assert_eq!(s, StatusCode::OK, "upload asset: {body}");
    let id = body["id"].as_str().unwrap().to_owned();
    assert_eq!(body["content_type"], "image/png");
    assert_eq!(body["size"], png.len());

    // Serve the bytes back (open read) with the stored content type.
    let (s, ct, bytes) = send_raw(&app, get(&format!("/assets/{id}"))).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(ct, "image/png");
    assert_eq!(bytes, png);

    // Re-upload identical bytes → dedup to the same id, one row.
    let (s, body) = send(&app, upload(png.clone(), "logo-copy.png")).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body["id"], id, "dedup returns the first asset");
    let (s, list) = send(&app, get("/assets")).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(list.as_array().unwrap().len(), 1, "one row, not two");

    // Delete.
    let (s, body) = send(&app, loop_json("DELETE", &format!("/assets/{id}"), Value::Null)).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body["deleted"], true);
    let (s, _, _) = send_raw(&app, get(&format!("/assets/{id}"))).await;
    assert_eq!(s, StatusCode::NOT_FOUND, "deleted asset is gone");
}

#[tokio::test]
async fn document_crud_references_render_export() {
    let app = app().await;

    // A reusable reference document (T&C) and a main document — both are now
    // containers; their content lives in pages.
    let (s, _) = send(
        &app,
        loop_json(
            "POST",
            "/documents",
            json!({"id":"tc","title":"Terms","kind":"reference"}),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let (s, body) = send(
        &app,
        loop_json("POST", "/documents", json!({"id":"quote","title":"Quote"})),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "create document: {body}");
    assert_eq!(body["kind"], "document");

    // Add the T&C content as a page on the reference document.
    let (s, _) = send(
        &app,
        loop_json(
            "POST",
            "/documents/tc/pages",
            json!({"title":"Terms","body":"## Terms\n\nBe nice."}),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK);

    // Author the quote's content across two pages; appended in order.
    let (s, page1) = send(
        &app,
        loop_json(
            "POST",
            "/documents/quote/pages",
            json!({"title":"Cover","body":"# Quote\n\nPrice: $10."}),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "create page: {page1}");
    let p1 = page1["id"].as_str().unwrap().to_owned();
    let (s, _) = send(
        &app,
        loop_json(
            "POST",
            "/documents/quote/pages",
            json!({"title":"Details","body":"## Details\n\nNet 30."}),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK);

    // List pages: two, in position order.
    let (s, pages) = send(&app, get("/documents/quote/pages")).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(pages.as_array().unwrap().len(), 2);
    assert_eq!(pages[0]["title"], "Cover");
    assert_eq!(pages[1]["title"], "Details");

    // Edit a page (body + move it after the other by writing a later position).
    let (s, edited) = send(
        &app,
        loop_json(
            "PUT",
            &format!("/documents/quote/pages/{p1}"),
            json!({"title":"Cover","body":"# Quote\n\nPrice: $10.","position":99.0}),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "edit page: {edited}");
    let (s, pages) = send(&app, get("/documents/quote/pages")).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(pages[0]["title"], "Details", "reorder moved Cover to the end");

    // Open reads: get + list documents.
    let (s, body) = send(&app, get("/documents")).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body.as_array().unwrap().len(), 2);

    // Attach the reference; it appears in the references listing.
    let (s, _) = send(
        &app,
        loop_json(
            "POST",
            "/documents/quote/references",
            json!({"reference_id":"tc"}),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let (s, refs) = send(&app, get("/documents/quote/references")).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(refs.as_array().unwrap().len(), 1);
    assert_eq!(refs[0]["thing_id"], "tc");

    // Render: HTML preview contains both pages AND the merged reference.
    let (s, ct, bytes) = send_raw(&app, get("/documents/quote/render")).await;
    assert_eq!(s, StatusCode::OK);
    assert!(ct.starts_with("text/html"), "render is HTML: {ct}");
    let html = String::from_utf8(bytes).unwrap();
    assert!(html.contains("Price: $10."), "page 1 present: {html}");
    assert!(html.contains("Net 30."), "page 2 present");
    assert!(html.contains("Be nice."), "merged reference present");
    // Each saved page is its own A4 sheet (two quote pages + one merged
    // reference): the preview must paginate, not collapse onto one sheet.
    assert_eq!(
        html.matches("class=\"doc-page\"").count(),
        3,
        "one preview sheet per saved page (2 quote + 1 reference)"
    );

    // Export: a real PDF (application/pdf, %PDF header).
    let (s, ct, bytes) = send_raw(&app, get("/documents/quote/export.pdf")).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(ct, "application/pdf");
    assert!(bytes.starts_with(b"%PDF"), "looks like a PDF");
    // …and the PDF carries one page object per saved page (page-broken), not a
    // single page with everything stacked.
    let pdf = String::from_utf8_lossy(&bytes);
    let page_objects =
        pdf.matches("/Type /Page\n").count() + pdf.matches("/Type/Page").count();
    assert!(
        page_objects >= 3,
        "PDF should have one page per saved page (got {page_objects})"
    );

    // Update preserves the id; delete reports existence.
    let (s, body) = send(
        &app,
        loop_json("PUT", "/documents/quote", json!({"title":"Quote v2"})),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body["title"], "Quote v2");

    // Delete a single page; the other remains.
    let (s, body) = send(
        &app,
        loop_json("DELETE", &format!("/documents/quote/pages/{p1}"), Value::Null),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body["deleted"], true);
    let (s, pages) = send(&app, get("/documents/quote/pages")).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(pages.as_array().unwrap().len(), 1, "one page left after delete");

    // Deleting the document cascades its remaining pages.
    let (s, body) = send(&app, loop_json("DELETE", "/documents/quote", Value::Null)).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body["deleted"], true);
    let (s, _) = send(&app, get("/documents/quote/pages")).await;
    assert_eq!(s, StatusCode::NOT_FOUND, "pages 404 once the document is gone");

    // A reference detach + missing-document 404.
    let (s, _) = send(&app, get("/documents/ghost/references")).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}

/// The page-number / index layout toggles persist on the document (so they
/// survive a reload) and drive the render without a query override.
#[tokio::test]
async fn layout_toggles_persist_and_drive_render() {
    let app = app().await;

    let (s, _) = send(
        &app,
        loop_json("POST", "/documents", json!({"id":"lay","title":"Lay","kind":"document"})),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let (s, _) = send(
        &app,
        loop_json("POST", "/documents/lay/pages", json!({"title":"One","body":"# One"})),
    )
    .await;
    assert_eq!(s, StatusCode::OK);

    // Defaults are off: a fresh document persists `false` and renders no index /
    // page counter.
    let (s, doc) = send(&app, get("/documents/lay")).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(doc["page_numbers"], false);
    assert_eq!(doc["index"], false);

    // Save the toggles on the document (no render query).
    let (s, doc) = send(
        &app,
        loop_json(
            "PUT",
            "/documents/lay",
            json!({"title":"Lay","page_numbers":true,"index":true}),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(doc["page_numbers"], true, "page_numbers persisted: {doc}");
    assert_eq!(doc["index"], true, "index persisted");

    // They survive a fresh fetch (reload).
    let (s, doc) = send(&app, get("/documents/lay")).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(doc["page_numbers"], true);
    assert_eq!(doc["index"], true);

    // …and the render reflects them with NO query override (the saved values
    // drive it): the index page and the page counter both appear.
    let (s, ct, bytes) = send_raw(&app, get("/documents/lay/render")).await;
    assert_eq!(s, StatusCode::OK);
    assert!(ct.starts_with("text/html"));
    let html = String::from_utf8(bytes).unwrap();
    // Match the rendered markup, not the always-present CSS class definitions.
    assert!(html.contains("class=\"doc-index\""), "saved index toggle renders the index");
    assert!(html.contains("class=\"page-num\""), "saved page-number toggle renders the counter");

    // A query override still wins (the editor's live, unsaved checkbox state):
    // forcing index off hides it even though the saved value is on.
    let (s, _ct, bytes) = send_raw(&app, get("/documents/lay/render?index=false")).await;
    assert_eq!(s, StatusCode::OK);
    let html = String::from_utf8(bytes).unwrap();
    assert!(!html.contains("class=\"doc-index\""), "query override hides the index");
}

/// An empty page with the "page break" toggle on still renders as its own sheet
/// (a deliberate blank spacer), while an empty page with it off is dropped.
#[tokio::test]
async fn empty_page_with_page_break_still_renders() {
    let app = app().await;

    let (s, _) = send(
        &app,
        loop_json("POST", "/documents", json!({"id":"book","title":"Book","kind":"document"})),
    )
    .await;
    assert_eq!(s, StatusCode::OK);

    // A normal first page, a deliberately-blank spacer page (page_break on), and a
    // third page. The spacer must survive into the render even though it's empty.
    for body in [
        json!({"title":"One","body":"# One"}),
        json!({"title":"Spacer","body":"","page_break":true}),
        json!({"title":"Three","body":"# Three"}),
    ] {
        let (s, _) = send(&app, loop_json("POST", "/documents/book/pages", body)).await;
        assert_eq!(s, StatusCode::OK);
    }

    // The kept blank spacer is its own preview sheet → three sheets, not two.
    let (s, ct, bytes) = send_raw(&app, get("/documents/book/render")).await;
    assert_eq!(s, StatusCode::OK);
    assert!(ct.starts_with("text/html"));
    let html = String::from_utf8(bytes).unwrap();
    assert_eq!(
        html.matches("class=\"doc-page\"").count(),
        3,
        "the empty page_break page renders as its own sheet"
    );

    // …and the PDF carries a page object for it too.
    let (s, _ct, bytes) = send_raw(&app, get("/documents/book/export.pdf")).await;
    assert_eq!(s, StatusCode::OK);
    let pdf = String::from_utf8_lossy(&bytes);
    let page_objects = pdf.matches("/Type /Page\n").count() + pdf.matches("/Type/Page").count();
    assert!(page_objects >= 3, "kept blank page adds a PDF page (got {page_objects})");

    // Now add an empty page with the toggle OFF: it is dropped from the render.
    let (s, _) = send(
        &app,
        loop_json("POST", "/documents/book/pages", json!({"title":"Ghost","body":"","page_break":false})),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let (s, _ct, bytes) = send_raw(&app, get("/documents/book/render")).await;
    assert_eq!(s, StatusCode::OK);
    let html = String::from_utf8(bytes).unwrap();
    assert_eq!(
        html.matches("class=\"doc-page\"").count(),
        3,
        "an empty page with page_break off does not render"
    );
}

#[tokio::test]
async fn document_sources_link() {
    let app = app().await;
    let (s, _) = send(
        &app,
        loop_json("POST", "/documents", json!({"id":"doc","title":"Doc"})),
    )
    .await;
    assert_eq!(s, StatusCode::OK);

    // Add a link source (JSON body).
    let (s, body) = send(
        &app,
        loop_json(
            "POST",
            "/documents/doc/sources",
            json!({"url":"https://example.com/spec","title":"Spec"}),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "add link source: {body}");
    assert_eq!(body["kind"], "link");
    assert_eq!(body["url"], "https://example.com/spec");
    let sid = body["id"].as_str().unwrap().to_owned();

    // List + delete.
    let (s, list) = send(&app, get("/documents/doc/sources")).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(list.as_array().unwrap().len(), 1);
    let (s, body) = send(
        &app,
        loop_json("DELETE", &format!("/documents/doc/sources/{sid}"), Value::Null),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body["deleted"], true);

    // Sources mutation needs a token.
    let anon = Request::builder()
        .method("POST")
        .uri("/documents/doc/sources")
        .header("content-type", "application/json")
        .body(Body::from(json!({"url":"x"}).to_string()))
        .unwrap();
    let (s, _) = send(&app, anon).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn set_repo_persists_target() {
    let app = app().await;
    let (s, _) = send(
        &app,
        loop_json("POST", "/documents", json!({"id":"doc","title":"Doc"})),
    )
    .await;
    assert_eq!(s, StatusCode::OK);

    let (s, body) = send(
        &app,
        loop_json(
            "PUT",
            "/documents/doc/repo",
            json!({"repo":"/tmp/repo","output_path":"docs/doc.md","base_branch":"main"}),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "set repo: {body}");
    assert_eq!(body["repo"]["repo"], "/tmp/repo");
    assert_eq!(body["repo"]["output_path"], "docs/doc.md");

    // A gh action with no repo set on a *different* doc → 400.
    let (s, _) = send(
        &app,
        loop_json("POST", "/documents", json!({"id":"bare","title":"Bare"})),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let (s, _) = send(&app, loop_json("POST", "/documents/bare/gh/pr", json!({}))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "no repo target → 400");
}
