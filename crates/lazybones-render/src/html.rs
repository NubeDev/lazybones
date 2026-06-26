//! `render_html` — markdown → a standalone, brand-styled HTML page for the
//! in-UI preview.
//!
//! This is the **cheap fallback path**: it leans on `pulldown-cmark`'s built-in
//! HTML writer for the body and wraps it in a `<style>` block derived from the
//! brand palette/fonts, plus the optional header/footer. It never fails and pulls
//! in no Typst machinery, so the export route can always serve a preview even if
//! PDF rendering has a problem.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use pulldown_cmark::{CowStr, Event, Options, Parser, Tag, html};

use crate::model::{Assembled, ImageAsset};

/// Render an assembled document to a self-contained HTML page with brand CSS.
///
/// The logo and any inline images are embedded as `data:` URIs (their bytes
/// travel in [`Assembled`]). The in-UI preview shows them in an iframe rendered
/// from `srcDoc`, which has no base URL — relative `/assets/<id>` paths would not
/// resolve — so inlining is what makes the brand logo and images actually visible
/// in the preview.
#[must_use]
pub fn render_html(assembled: &Assembled) -> String {
    let c = &assembled.brand.colors;
    let f = &assembled.brand.fonts;
    let bg = css_or(&c.background, "#ffffff");
    let text = css_or(&c.text, "#1a1a1a");
    let primary = css_or(&c.primary, "#222222");
    let accent = css_or(&c.accent, "#2563eb");
    let heading_font = css_or(&f.heading, "Georgia, 'Times New Roman', serif");
    let body_font = css_or(&f.body, "Georgia, 'Times New Roman', serif");

    // The cover (header band, logo, title) is the first sheet's leading content —
    // exactly as the PDF template lays it out before the first page's body — so it
    // prints *on* the A4 page rather than floating on the grey desk above it.
    // The cover mirrors the PDF: logo, a small accent "eyebrow" (the brand header
    // text in caps, or "DOCUMENT" as a fallback), the oversized title, and a short
    // accent rule beneath it.
    let mut cover = String::new();
    cover.push_str("<div class=\"doc-cover\">\n");
    if let Some(logo) = &assembled.logo {
        cover.push_str(&format!(
            "<img class=\"brand-logo\" src=\"{}\" alt=\"\">\n",
            data_uri(logo)
        ));
    }
    let eyebrow = if assembled.brand.header_text.trim().is_empty() {
        "DOCUMENT".to_owned()
    } else {
        assembled.brand.header_text.trim().to_uppercase()
    };
    cover.push_str(&format!(
        "<p class=\"doc-eyebrow\">{}</p>\n",
        escape_html(&eyebrow)
    ));
    cover.push_str(&format!(
        "<h1 class=\"doc-title\">{}</h1>\n",
        escape_html(&assembled.title)
    ));
    cover.push_str("<div class=\"doc-rule\"></div>\n");
    cover.push_str("</div>\n");

    // When enabled, an index (table of contents) sheet rides on the cover page,
    // listing each non-empty page's title — mirroring the PDF, where the index
    // shares the title page and the body starts after a break.
    if assembled.options.index {
        cover.push_str(&index_html(assembled));
    }

    let brand_footer = if assembled.brand.footer_text.trim().is_empty() {
        String::new()
    } else {
        format!("<span class=\"brand-footer\">{}</span>", escape_html(&assembled.brand.footer_text))
    };

    // Render each page to its own A4 sheet (one card per page), so the preview
    // mirrors the paginated PDF. The cover rides on the first sheet; every sheet
    // gets a footer band carrying the brand footer (last page) and, when enabled,
    // a "page / total" counter — just like the printed document.
    let total = assembled.pages.len().max(1);
    let last = total - 1;
    let mut sections = String::new();
    for (i, page) in assembled.pages.iter().enumerate() {
        let lead = if i == 0 { cover.as_str() } else { "" };
        let band = page_footer_html(
            if i == last { &brand_footer } else { "" },
            assembled.options.page_numbers,
            i + 1,
            total,
        );
        sections.push_str(&format!(
            "<section class=\"doc-page\">\n{lead}{}{band}</section>\n",
            render_body(page, &assembled.images)
        ));
    }
    // A document with no pages still shows its cover on a single sheet.
    if assembled.pages.is_empty() {
        let band = page_footer_html(&brand_footer, assembled.options.page_numbers, 1, 1);
        sections.push_str(&format!("<section class=\"doc-page\">\n{cover}{band}</section>\n"));
    }

    format!(
        "<!doctype html>\n<html lang=\"en\"><head><meta charset=\"utf-8\">\n\
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
<title>{title}</title>\n<style>\n\
/* The preview mirrors the printed PDF: every page is a real A4 sheet \
(210mm x 297mm) with the same 2.2cm side / 2.6cm top / 2.4cm bottom margins \
as the Typst template, laid out on a grey desk and centred. */\n\
:root{{--page-w:210mm;--page-h:297mm;--page-mx:2.2cm;--page-mt:2.4cm;--page-mb:2.2cm;\
--primary:{primary};--accent:{accent};--text:{text};}}\n\
*{{box-sizing:border-box;}}\n\
body{{background:#e6e6e9;color:{text};font-family:{body_font};line-height:1.6;\
margin:0;padding:1.5rem;display:flex;flex-direction:column;align-items:center;gap:1.5rem;}}\n\
h1,h2,h3,h4,h5,h6{{font-family:{heading_font};color:{primary};line-height:1.25;\
margin:1.4em 0 0.5em;}}\n\
h2{{font-size:1.25rem;}}\n\
h3{{font-size:1.05rem;color:{accent};}}\n\
/* The cover mirrors the PDF: an accent eyebrow, an oversized title, and a short \
accent rule beneath it (replacing the old full-width underline). */\n\
.doc-cover{{margin:0 0 1.5rem;}}\n\
.doc-eyebrow{{font-size:0.72rem;font-weight:700;letter-spacing:0.18em;color:{accent};\
text-transform:uppercase;margin:0 0 0.5rem;}}\n\
.doc-title{{font-size:2rem;font-weight:700;margin:0 0 0.6rem;line-height:1.15;}}\n\
.doc-rule{{width:3.5cm;height:3px;background:{accent};border-radius:1px;}}\n\
/* The footer band pins to the bottom of its sheet (flex push) and carries the \
brand footer on the left and the page counter on the right, above a hairline. */\n\
.page-foot{{margin-top:auto;display:flex;justify-content:space-between;align-items:flex-end;\
gap:1rem;padding-top:0.6rem;border-top:1px solid rgba(0,0,0,0.12);opacity:0.7;font-size:0.78rem;}}\n\
.page-foot .page-num{{margin-left:auto;font-variant-numeric:tabular-nums;}}\n\
/* Index (table of contents): index number in the accent color, dotted leaders. */\n\
.doc-index{{margin:0 0 1rem;}}\n\
.doc-index h2{{margin:0 0 0.4rem;}}\n\
.doc-index ol{{list-style:none;margin:0;padding:0;border-top:1px solid rgba(0,0,0,0.12);\
padding-top:0.7rem;}}\n\
.doc-index li{{display:flex;gap:0.6em;padding:0.2rem 0;align-items:baseline;}}\n\
.doc-index li::after{{content:\"\";flex:1;border-bottom:1px dotted rgba(0,0,0,0.3);\
margin-bottom:0.2em;}}\n\
.doc-index .idx-n{{color:{accent};font-weight:700;min-width:1.4em;}}\n\
.brand-logo{{max-height:3.2rem;width:auto;margin-bottom:1.2rem;}}\n\
/* The sheet keeps true A4 proportions but shrinks to fit a narrow preview pane \
(the iframe is often half the screen) so it never clips horizontally. \
aspect-ratio holds 210:297 once the width caps below A4. The flex column lets the \
footer band sink to the bottom of every sheet. */\n\
.doc-page{{background:{bg};color:{text};box-shadow:0 2px 10px rgba(0,0,0,0.22);\
width:min(var(--page-w),100%);aspect-ratio:210/297;display:flex;flex-direction:column;\
padding:var(--page-mt) var(--page-mx) var(--page-mb);margin:0;overflow:hidden;}}\n\
@media(min-width:840px){{.doc-page{{width:var(--page-w);height:var(--page-h);aspect-ratio:auto;}}}}\n\
.doc-page>*:first-child{{margin-top:0;}}\n\
.doc-page p{{text-align:justify;}}\n\
a{{color:{accent};text-decoration:underline;}}\n\
code,pre{{font-family:ui-monospace,SFMono-Regular,Menlo,monospace;}}\n\
code{{font-size:0.88em;}}\n\
/* Block code in a tinted, padded panel matching the PDF. */\n\
pre{{background:{primary}10;padding:0.85rem 1rem;border-radius:5px;overflow:auto;\
font-size:0.82rem;line-height:1.45;}}\n\
pre code{{font-size:inherit;}}\n\
/* Block quote: accent rule on the left, muted italic text. */\n\
blockquote{{border-left:2px solid {accent};margin:1em 0;padding:0.1rem 0 0.1rem 1rem;\
font-style:italic;opacity:0.8;}}\n\
/* Table: filled header row (primary), white bold header text, zebra body rows, \
soft hairline row separators — no heavy cell borders. */\n\
table{{border-collapse:collapse;width:100%;margin:1em 0;font-size:0.95em;}}\n\
thead th{{background:{primary};color:#fff;font-weight:700;text-align:left;}}\n\
th,td{{padding:0.5rem 0.8rem;border:none;}}\n\
tbody tr{{border-top:1px solid rgba(0,0,0,0.12);}}\n\
tbody tr:nth-child(even){{background:{primary}0d;}}\n\
img{{max-width:100%;}}\n</style>\n</head>\n<body>\n{sections}</body></html>",
        title = escape_html(&assembled.title),
    )
}

/// The index (table-of-contents) fragment: a heading and a numbered list of each
/// page's title, in render order. Every page the caller passes is a real page (a
/// deliberately-blank spacer included), so all are listed. Returns `""` when
/// there are no pages.
fn index_html(assembled: &Assembled) -> String {
    let mut rows = String::new();
    for (i, _page) in assembled.pages.iter().enumerate() {
        rows.push_str(&format!(
            "<li><span class=\"idx-n\">{}</span>{}</li>\n",
            i + 1,
            escape_html(&assembled.page_label(i))
        ));
    }
    if rows.is_empty() {
        return String::new();
    }
    format!("<section class=\"doc-index\">\n<h2>Index</h2>\n<ol>\n{rows}</ol>\n</section>\n")
}

/// A sheet's footer band: the brand footer (already HTML) on the left and, when
/// enabled, a `page / total` counter on the right. Returns `""` when neither is
/// present so an unconfigured sheet keeps no footer.
fn page_footer_html(brand: &str, page_numbers: bool, page: usize, total: usize) -> String {
    if brand.is_empty() && !page_numbers {
        return String::new();
    }
    let number = if page_numbers {
        format!("<span class=\"page-num\">{page} / {total}</span>")
    } else {
        String::new()
    };
    format!("<footer class=\"page-foot\">{brand}{number}</footer>\n")
}

/// Render one page's markdown to an HTML fragment, rewriting inline image `src`s
/// to `data:` URIs so they render in the (base-URL-less) preview iframe.
fn render_body(markdown: &str, images: &[ImageAsset]) -> String {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_TASKLISTS);

    let parser = Parser::new_ext(markdown, options).map(|event| match event {
        Event::Start(Tag::Image {
            link_type,
            dest_url,
            title,
            id,
        }) => {
            let dest_url = data_uri_for_src(images, &dest_url).map_or(dest_url, CowStr::from);
            Event::Start(Tag::Image {
                link_type,
                dest_url,
                title,
                id,
            })
        }
        other => other,
    });

    let mut body = String::new();
    html::push_html(&mut body, parser);
    body
}

/// The `data:` URI for the inline image whose `src` matches `src`, or `None` if
/// no resolved image satisfies it (the original `src` is then kept as-is).
fn data_uri_for_src(images: &[ImageAsset], src: &str) -> Option<String> {
    images.iter().find(|i| i.src == src).map(data_uri)
}

/// Encode a resolved image as a `data:<mime>;base64,...` URI.
fn data_uri(image: &ImageAsset) -> String {
    format!(
        "data:{};base64,{}",
        mime_for(&image.filename),
        BASE64.encode(&image.bytes)
    )
}

/// The image MIME type keyed off a filename's extension; defaults to `image/png`.
fn mime_for(filename: &str) -> &'static str {
    let ext = filename
        .rsplit('.')
        .next()
        .filter(|e| !e.is_empty() && *e != filename)
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "webp" => "image/webp",
        _ => "image/png",
    }
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
    use crate::model::{Brand, Colors, ImageAsset};

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
        // The brand header text becomes the uppercased cover eyebrow.
        assert!(html.contains("class=\"doc-eyebrow\">CONFIDENTIAL</p>"));
        assert!(html.contains("brand-footer\">&copy; ACME</span>") || html.contains("brand-footer\">© ACME</span>"));
    }

    #[test]
    fn embeds_the_logo_as_a_data_uri() {
        let assembled = Assembled::new("Branded", "Body")
            .with_logo(ImageAsset::new("", "logo.png", vec![1, 2, 3, 4]));
        let html = render_html(&assembled);
        assert!(html.contains("class=\"brand-logo\""));
        assert!(html.contains("src=\"data:image/png;base64,"));
    }

    #[test]
    fn rewrites_inline_image_src_to_a_data_uri() {
        let assembled = Assembled::new("Doc", "![alt](/assets/abc)")
            .with_image(ImageAsset::new("/assets/abc", "pic.jpg", vec![9, 9, 9]));
        let html = render_html(&assembled);
        assert!(html.contains("data:image/jpeg;base64,"));
        // The unresolved server path must not leak into the preview.
        assert!(!html.contains("/assets/abc"));
    }

    #[test]
    fn each_page_renders_as_its_own_card() {
        let assembled = Assembled::with_pages(
            "Book",
            vec!["# Page one".to_owned(), "# Page two".to_owned()],
        );
        let html = render_html(&assembled);
        assert_eq!(html.matches("class=\"doc-page\"").count(), 2, "one card per page");
        assert!(html.contains("Page one"));
        assert!(html.contains("Page two"));
    }

    #[test]
    fn pages_are_sized_as_real_a4_sheets() {
        let html = render_html(&Assembled::new("Doc", "body"));
        // The preview must lay each page out at true A4 dimensions, not an
        // arbitrary min-height, so it matches the printed PDF.
        assert!(html.contains("--page-w:210mm"));
        assert!(html.contains("--page-h:297mm"));
        assert!(html.contains("width:var(--page-w)"));
        assert!(html.contains("height:var(--page-h)"));
        assert!(!html.contains("min-height:60vh"));
    }

    #[test]
    fn cover_rides_on_the_first_sheet() {
        // The title (and header/logo) belong inside the first A4 sheet, mirroring
        // the PDF, rather than floating above the pages on the grey desk.
        let assembled = Assembled::with_pages(
            "My Title",
            vec!["# One".to_owned(), "# Two".to_owned()],
        )
        .with_brand(Brand {
            header_text: "Confidential".into(),
            ..Brand::default()
        });
        let html = render_html(&assembled);
        let first = html.split("<section class=\"doc-page\">").nth(1).unwrap();
        let first_page = first.split("</section>").next().unwrap();
        assert!(first_page.contains("doc-title"), "title is on the first sheet");
        assert!(first_page.contains("class=\"doc-eyebrow\">CONFIDENTIAL</p>"));
    }

    #[test]
    fn page_numbers_add_a_counter_to_every_sheet() {
        use crate::model::RenderOptions;
        let assembled = Assembled::with_pages("Book", vec!["a".to_owned(), "b".to_owned()])
            .with_options(RenderOptions {
                page_numbers: true,
                index: false,
            });
        let html = render_html(&assembled);
        assert!(html.contains("class=\"page-num\">1 / 2<"));
        assert!(html.contains("class=\"page-num\">2 / 2<"));
    }

    #[test]
    fn no_options_means_no_counter_or_index() {
        let html = render_html(&Assembled::new("Doc", "body"));
        // No rendered counter span and no index section (the CSS class definitions
        // for these always exist; assert on the rendered markup, not the styles).
        assert!(!html.contains("class=\"page-num\">"));
        assert!(!html.contains("class=\"doc-index\">"));
        assert!(!html.contains("class=\"page-foot\">"));
    }

    #[test]
    fn index_lists_every_page_title_including_blanks() {
        use crate::model::RenderOptions;
        // A blank spacer page is a real page and is listed (the caller already
        // dropped any page that shouldn't render).
        let assembled = Assembled::with_pages(
            "Book",
            vec!["One".to_owned(), "   ".to_owned(), "Three".to_owned()],
        )
        .with_page_titles(vec!["Alpha".to_owned(), "Spacer".to_owned(), "Gamma".to_owned()])
        .with_options(RenderOptions {
            page_numbers: false,
            index: true,
        });
        let html = render_html(&assembled);
        assert!(html.contains("doc-index"));
        assert!(html.contains(">Alpha<") || html.contains("Alpha</li>"));
        assert!(html.contains("Spacer"));
        assert!(html.contains("Gamma"));
    }

    #[test]
    fn index_falls_back_to_generic_labels_without_titles() {
        use crate::model::RenderOptions;
        let assembled = Assembled::with_pages("Book", vec!["body".to_owned()])
            .with_options(RenderOptions { page_numbers: false, index: true });
        let html = render_html(&assembled);
        assert!(html.contains("Page 1"));
    }

    #[test]
    fn escapes_title_html() {
        let assembled = Assembled::new("A <script> & \"x\"", "body");
        let html = render_html(&assembled);
        assert!(html.contains("&lt;script&gt;"));
        assert!(!html.contains("<script>"));
    }
}
