//! `render_html` — markdown → a standalone, brand-styled HTML page for the
//! in-UI preview.
//!
//! This is the **cheap fallback path**: it leans on `pulldown-cmark`'s built-in
//! HTML writer for the body and wraps it in a `<style>` block derived from the
//! brand palette/fonts, plus the optional header/footer. It never fails and pulls
//! in no Typst machinery, so the export route can always serve a preview even if
//! PDF rendering has a problem.

use pulldown_cmark::{Options, Parser, html};

use crate::model::Assembled;

/// Render an assembled document to a self-contained HTML page with brand CSS.
#[must_use]
pub fn render_html(assembled: &Assembled) -> String {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_TASKLISTS);

    let mut body = String::new();
    html::push_html(&mut body, Parser::new_ext(&assembled.markdown, options));

    let c = &assembled.brand.colors;
    let f = &assembled.brand.fonts;
    let bg = css_or(&c.background, "#ffffff");
    let text = css_or(&c.text, "#1a1a1a");
    let primary = css_or(&c.primary, "#222222");
    let accent = css_or(&c.accent, "#2563eb");
    let heading_font = css_or(&f.heading, "Georgia, 'Times New Roman', serif");
    let body_font = css_or(&f.body, "Georgia, 'Times New Roman', serif");

    let mut sections = String::new();
    if !assembled.brand.header_text.trim().is_empty() {
        sections.push_str(&format!(
            "<header>{}</header>\n",
            escape_html(&assembled.brand.header_text)
        ));
    }
    sections.push_str(&format!("<h1 class=\"doc-title\">{}</h1>\n", escape_html(&assembled.title)));
    sections.push_str(&body);
    if !assembled.brand.footer_text.trim().is_empty() {
        sections.push_str(&format!(
            "<footer>{}</footer>\n",
            escape_html(&assembled.brand.footer_text)
        ));
    }

    format!(
        "<!doctype html>\n<html lang=\"en\"><head><meta charset=\"utf-8\">\n\
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
<title>{title}</title>\n<style>\n\
body{{background:{bg};color:{text};font-family:{body_font};line-height:1.6;\
margin:0 auto;max-width:46rem;padding:2.5rem 1.5rem;}}\n\
h1,h2,h3,h4,h5,h6{{font-family:{heading_font};color:{primary};line-height:1.25;}}\n\
.doc-title{{border-bottom:2px solid {primary};padding-bottom:0.3rem;}}\n\
a{{color:{accent};}}\n\
code,pre{{font-family:ui-monospace,SFMono-Regular,Menlo,monospace;}}\n\
pre{{background:rgba(0,0,0,0.05);padding:0.75rem 1rem;border-radius:6px;overflow:auto;}}\n\
blockquote{{border-left:3px solid {primary};margin-left:0;padding-left:1rem;opacity:0.85;}}\n\
table{{border-collapse:collapse;}}\n\
th,td{{border:1px solid rgba(0,0,0,0.2);padding:0.4rem 0.7rem;}}\n\
header,footer{{opacity:0.65;font-size:0.85rem;}}\n\
img{{max-width:100%;}}\n</style>\n</head>\n<body>\n{sections}</body></html>",
        title = escape_html(&assembled.title),
    )
}

/// A CSS value, or `fallback` when the brand left the field blank.
fn css_or(value: &str, fallback: &str) -> String {
    if value.trim().is_empty() {
        fallback.to_owned()
    } else {
        value.trim().to_owned()
    }
}

/// HTML-escape the five significant characters (used for title/header/footer,
/// which are plain text, not markdown).
fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Brand, Colors};

    #[test]
    fn renders_markdown_body_into_html() {
        let assembled = Assembled::new("My Doc", "# Hi\n\nSome **bold** text.");
        let html = render_html(&assembled);
        assert!(html.contains("<title>My Doc</title>"));
        assert!(html.contains("<strong>bold</strong>"));
        assert!(html.contains("doc-title"));
    }

    #[test]
    fn applies_brand_palette_and_bands() {
        let brand = Brand {
            colors: Colors {
                primary: "#1d4ed8".into(),
                accent: "#db2777".into(),
                ..Colors::default()
            },
            header_text: "Confidential".into(),
            footer_text: "© ACME".into(),
            ..Brand::default()
        };
        let assembled = Assembled::new("Branded", "Body").with_brand(brand);
        let html = render_html(&assembled);
        assert!(html.contains("#1d4ed8"));
        assert!(html.contains("#db2777"));
        assert!(html.contains("<header>Confidential</header>"));
        assert!(html.contains("<footer>&copy; ACME</footer>") || html.contains("<footer>© ACME</footer>"));
    }

    #[test]
    fn escapes_title_html() {
        let assembled = Assembled::new("A <script> & \"x\"", "body");
        let html = render_html(&assembled);
        assert!(html.contains("&lt;script&gt;"));
        assert!(!html.contains("<script>"));
    }
}
