//! `GET /documents/:id/render` (assembled HTML preview) and
//! `GET /documents/:id/export.pdf` (PDF).
//!
//! Assembly — resolving attached `reference` pages into one markdown blob and
//! resolving the brand profile — lives here in the API (keeping the future
//! `lazybones-render` crate pure). The HTML preview is the cheap path; the PDF is
//! a self-contained placeholder writer until the Typst render layer (Phase 3b)
//! lands behind this same route. Both reads are open, like the rest of `/tasks`.

use axum::extract::{Path, State};
use axum::http::header::CONTENT_TYPE;
use axum::response::{Html, IntoResponse, Response};
use lazybones_store::{Branding, Document};

use crate::error::ApiResult;
use crate::routes::documents::{REFERENCE_KIND, require_document};
use crate::state::AppState;

/// A document resolved into renderable inputs: its title, the assembled markdown
/// (body + merged references, in attach order), and the resolved brand profile.
struct Assembled {
    title: String,
    markdown: String,
    branding: Option<Branding>,
}

/// Assemble a document's renderable markdown: its body followed by each attached
/// `reference` document's body, in attach order (`list_attachments` returns
/// newest-first, so we reverse). Shared with the GitHub commit route.
pub(crate) async fn assemble_markdown(state: &AppState, doc: &Document) -> ApiResult<String> {
    let mut markdown = doc.body.clone();
    let mut refs = state
        .store
        .list_attachments("document", &doc.id, Some(REFERENCE_KIND))
        .await?;
    refs.reverse(); // attach order: oldest first
    for att in refs {
        if let Some(reference) = state.store.get_document(&att.thing_id).await? {
            markdown.push_str("\n\n");
            markdown.push_str(&reference.body);
        }
    }
    Ok(markdown)
}

/// Resolve a document into [`Assembled`]: its assembled markdown plus the resolved
/// brand profile.
async fn assemble(state: &AppState, doc: &Document) -> ApiResult<Assembled> {
    let markdown = assemble_markdown(state, doc).await?;
    let branding = match doc.branding_id.as_deref() {
        Some(id) => state.store.get_branding(id).await?,
        None => None,
    };
    Ok(Assembled {
        title: doc.title.clone(),
        markdown,
        branding,
    })
}

/// `GET /documents/:id/render` — the assembled HTML preview (body + merged
/// references), with brand colors/fonts applied as CSS. `404` if unknown.
pub async fn render_document(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Html<String>> {
    let doc = require_document(&state, &id).await?;
    let assembled = assemble(&state, &doc).await?;
    Ok(Html(render_html(&assembled)))
}

/// `GET /documents/:id/export.pdf` — the document as a PDF (`application/pdf`).
/// `404` if unknown.
pub async fn export_pdf(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Response> {
    let doc = require_document(&state, &id).await?;
    let assembled = assemble(&state, &doc).await?;
    let pdf = render_pdf(&assembled);
    Ok(([(CONTENT_TYPE, "application/pdf")], pdf).into_response())
}

/// Render the assembled document to a standalone HTML page, applying the brand
/// palette/fonts as CSS. The markdown→HTML pass is intentionally light (headings,
/// paragraphs) — the full converter ships with the Typst render layer.
fn render_html(a: &Assembled) -> String {
    let (bg, text, heading_font, body_font, header, footer) = match &a.branding {
        Some(b) => (
            css_or(&b.colors.background, "#ffffff"),
            css_or(&b.colors.text, "#111111"),
            css_or(&b.fonts.heading, "sans-serif"),
            css_or(&b.fonts.body, "sans-serif"),
            b.header_text.clone(),
            b.footer_text.clone(),
        ),
        None => (
            "#ffffff".to_owned(),
            "#111111".to_owned(),
            "sans-serif".to_owned(),
            "sans-serif".to_owned(),
            String::new(),
            String::new(),
        ),
    };
    let mut body = String::new();
    if !header.is_empty() {
        body.push_str(&format!("<header>{}</header>\n", escape_html(&header)));
    }
    body.push_str(&markdown_to_html(&a.markdown));
    if !footer.is_empty() {
        body.push_str(&format!("<footer>{}</footer>\n", escape_html(&footer)));
    }
    format!(
        "<!doctype html>\n<html><head><meta charset=\"utf-8\"><title>{title}</title>\n\
<style>body{{background:{bg};color:{text};font-family:{body_font};margin:2rem auto;max-width:48rem;}}\
h1,h2,h3{{font-family:{heading_font};}}header,footer{{opacity:0.7;font-size:0.9rem;}}</style>\n\
</head><body>\n<h1>{title}</h1>\n{body}</body></html>",
        title = escape_html(&a.title),
        bg = bg,
        text = text,
        body_font = body_font,
        heading_font = heading_font,
        body = body,
    )
}

/// A CSS value or a fallback if the brand left it blank.
fn css_or(value: &str, fallback: &str) -> String {
    if value.trim().is_empty() {
        fallback.to_owned()
    } else {
        value.to_owned()
    }
}

/// A deliberately small markdown→HTML pass: ATX headings and blank-line-separated
/// paragraphs, everything HTML-escaped. Good enough for an in-UI preview.
fn markdown_to_html(md: &str) -> String {
    let mut out = String::new();
    for block in md.split("\n\n") {
        let block = block.trim();
        if block.is_empty() {
            continue;
        }
        if let Some(rest) = block.strip_prefix("### ") {
            out.push_str(&format!("<h3>{}</h3>\n", escape_html(rest)));
        } else if let Some(rest) = block.strip_prefix("## ") {
            out.push_str(&format!("<h2>{}</h2>\n", escape_html(rest)));
        } else if let Some(rest) = block.strip_prefix("# ") {
            out.push_str(&format!("<h2>{}</h2>\n", escape_html(rest)));
        } else {
            out.push_str(&format!("<p>{}</p>\n", escape_html(block).replace('\n', "<br>")));
        }
    }
    out
}

/// HTML-escape the five significant characters.
fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// Render the assembled document to a minimal, valid single-stream PDF embedding
/// its text (title + assembled body). A self-contained placeholder until the
/// Typst-backed `render_pdf` (Phase 3b) replaces it behind the export route.
fn render_pdf(a: &Assembled) -> Vec<u8> {
    // Flatten to plain text lines: the title, then the assembled markdown with
    // heading markers stripped. Long lines are wrapped to a fixed column.
    let mut lines = vec![a.title.clone(), String::new()];
    for raw in a.markdown.lines() {
        let line = raw
            .trim_start_matches('#')
            .trim_start()
            .to_owned();
        if line.is_empty() {
            lines.push(String::new());
        } else {
            for chunk in wrap(&line, 88) {
                lines.push(chunk);
            }
        }
    }

    let mut content = String::from("BT\n/F1 11 Tf\n14 TL\n72 740 Td\n");
    for line in &lines {
        content.push_str(&format!("({}) Tj T*\n", pdf_escape(line)));
    }
    content.push_str("ET");
    let content = content.into_bytes();

    let objects: Vec<Vec<u8>> = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>".to_vec(),
        {
            let mut o = format!("<< /Length {} >>\nstream\n", content.len()).into_bytes();
            o.extend_from_slice(&content);
            o.extend_from_slice(b"\nendstream");
            o
        },
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_vec(),
    ];

    let mut out = Vec::new();
    out.extend_from_slice(b"%PDF-1.4\n");
    let mut offsets = Vec::with_capacity(objects.len());
    for (i, obj) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        out.extend_from_slice(obj);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref_pos = out.len();
    let size = objects.len() + 1;
    out.extend_from_slice(format!("xref\n0 {size}\n").as_bytes());
    out.extend_from_slice(b"0000000000 65535 f \n");
    for off in &offsets {
        out.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!("trailer\n<< /Size {size} /Root 1 0 R >>\nstartxref\n{xref_pos}\n%%EOF")
            .as_bytes(),
    );
    out
}

/// Escape a string for a PDF literal string `(...)` and drop non-printable bytes
/// so the stream stays well-formed.
fn pdf_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '(' => out.push_str("\\("),
            ')' => out.push_str("\\)"),
            c if c.is_ascii() && !c.is_control() => out.push(c),
            _ => out.push(' '),
        }
    }
    out
}

/// Greedily wrap `line` to at most `width` columns on whitespace.
fn wrap(line: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in line.split_whitespace() {
        if !current.is_empty() && current.len() + 1 + word.len() > width {
            lines.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
    }
    if !current.is_empty() {
        lines.push(current);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}
