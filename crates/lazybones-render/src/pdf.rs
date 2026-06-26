//! `render_pdf` — build a branded `.typ` from an [`Assembled`] document and
//! compile it to PDF bytes via the offline [`RenderWorld`](crate::world).
//!
//! The template applies the brand palette (page fill, body/heading text color,
//! link color), the brand fonts (with an embedded fallback so an unknown brand
//! font never fails compilation), an optional logo, the title, and the
//! header/footer. The document body comes from the markdown→Typst converter —
//! the part that carries the real risk; the template around it is deliberately
//! plain.

use typst_layout::PagedDocument;

use crate::convert::{markdown_to_typst, typst_string};
use crate::error::RenderError;
use crate::model::Assembled;
use crate::world::RenderWorld;

/// Render an assembled document to PDF bytes.
///
/// # Errors
/// Returns [`RenderError::Compile`] if the generated template fails to compile
/// (e.g. a malformed image), or [`RenderError::Pdf`] if PDF emission fails.
pub fn render_pdf(assembled: &Assembled) -> Result<Vec<u8>, RenderError> {
    // Register the logo + every inline image as a virtual file the template can
    // `image(...)`, and remember which markdown `src` maps to which path.
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    let logo_path = assembled.logo.as_ref().map(|logo| {
        let path = format!("logo.{}", image_ext(&logo.filename));
        files.push((path.clone(), logo.bytes.clone()));
        path
    });

    let mut image_paths: Vec<(String, String)> = Vec::new();
    for (i, img) in assembled.images.iter().enumerate() {
        let path = format!("img-{i}.{}", image_ext(&img.filename));
        files.push((path.clone(), img.bytes.clone()));
        image_paths.push((img.src.clone(), path));
    }

    let resolve = |src: &str| {
        image_paths
            .iter()
            .find(|(s, _)| s == src)
            .map(|(_, path)| path.clone())
    };
    // Convert each page independently and join with a real Typst page break, so
    // every document page lands on its own PDF page. An empty page is kept (it
    // converts to empty markup) so a deliberate blank spacer page still produces a
    // real page — the caller decides which pages reach here.
    let body = assembled
        .pages
        .iter()
        .map(|page| markdown_to_typst(page, |src| resolve(src)))
        .collect::<Vec<_>>()
        .join("\n#pagebreak()\n\n");

    let source = build_template(assembled, logo_path.as_deref(), &body);

    let world = RenderWorld::new(&source, &files);
    let compiled = typst::compile::<PagedDocument>(&world);
    let document = compiled
        .output
        .map_err(|diags| RenderError::Compile(format_diags(&diags)))?;

    typst_pdf::pdf(&document, &typst_pdf::PdfOptions::default())
        .map_err(|diags| RenderError::Pdf(format_diags(&diags)))
}

/// Assemble the branded `.typ` source around the converted body.
///
/// The layout is a professional, corporate document:
/// - a dedicated **cover page** (logo, oversized title, an accent rule, and the
///   brand header/footer text laid out as cover metadata),
/// - a **running header** carrying the brand header text under a hairline rule on
///   body pages (suppressed on the cover),
/// - a **footer** with the brand footer text on the left and a `page / total`
///   counter on the right, above a hairline rule (body pages only),
/// - typographic `show` rules that give headings a clear size/weight/color
///   hierarchy and style tables and code blocks (header fill, zebra rows, tinted
///   code panels) instead of leaving them flat.
fn build_template(a: &Assembled, logo_path: Option<&str>, body: &str) -> String {
    let c = &a.brand.colors;
    let f = &a.brand.fonts;

    let background = typst_color(&c.background, "white");
    let text_color = typst_color(&c.text, "rgb(\"#1a1a1a\")");
    let primary = typst_color(&c.primary, "rgb(\"#1a2b4a\")");
    let accent = typst_color(&c.accent, "rgb(\"#2563eb\")");
    let body_font = font_list(&f.body);
    let heading_font = font_list(&f.heading);
    // A muted ink derived for secondary text (header/footer, captions). Typst can
    // mix a color toward the page fill at layout time, so this stays correct
    // whatever the brand text color is.
    let muted = format!("{text_color}.lighten(35%)");
    // A faint tint of the primary for table header fills and code panels.
    let panel = format!("{primary}.lighten(92%)");
    let zebra = format!("{primary}.lighten(96%)");
    let rule = format!("{text_color}.lighten(75%)");

    let mut out = String::new();

    // ---- Page geometry + running header/footer (body pages) ---------------
    // The cover suppresses both bands via a page-local `#set page(..)` override,
    // so these only ever decorate body pages.
    let header = running_header(&a.brand.header_text, &muted, &rule);
    let footer = running_footer(&a.brand.footer_text, &muted, &rule, a.options.page_numbers);
    out.push_str(&format!(
        "#set page(paper: \"a4\", margin: (x: 2.2cm, top: 2.4cm, bottom: 2.2cm), fill: {background}, header: {header}, header-ascent: 40%, footer: {footer}, footer-descent: 40%)\n"
    ));

    // ---- Text + paragraph defaults ---------------------------------------
    out.push_str(&format!(
        "#set text(font: {body_font}, fill: {text_color}, size: 10.5pt)\n"
    ));
    out.push_str("#set par(justify: true, leading: 0.72em, spacing: 1.25em)\n");
    // Inline + block code in a real monospace face, slightly down-sized.
    out.push_str("#show raw: set text(font: (\"DejaVu Sans Mono\",), size: 9pt)\n");

    // ---- Heading hierarchy ------------------------------------------------
    out.push_str(&format!(
        "#show heading: set text(font: {heading_font}, fill: {primary})\n"
    ));
    out.push_str("#show heading: set block(above: 1.4em, below: 0.7em)\n");
    // h1: large, with a short accent underline drawn after the text.
    out.push_str(&format!(
        "#show heading.where(level: 1): it => block[#text(size: 17pt, weight: \"bold\")[#it.body] #v(-0.3em) #box(width: 2.2em, line(length: 100%, stroke: 2pt + {accent}))]\n"
    ));
    out.push_str("#show heading.where(level: 2): set text(size: 13pt, weight: \"bold\")\n");
    out.push_str(&format!(
        "#show heading.where(level: 3): set text(size: 11pt, weight: \"bold\", fill: {accent})\n"
    ));

    // ---- Links, quotes, rules, lists -------------------------------------
    out.push_str(&format!(
        "#show link: it => text(fill: {accent}, underline(it))\n"
    ));
    out.push_str(&format!(
        "#show quote.where(block: true): it => block(inset: (left: 1em), stroke: (left: 2pt + {accent}))[#text(style: \"italic\", fill: {muted})[#it.body]]\n"
    ));
    out.push_str("#set list(indent: 0.6em, spacing: 0.7em)\n");
    out.push_str("#set enum(indent: 0.6em, spacing: 0.7em)\n");

    // ---- Table styling: filled header row, zebra body, soft hairlines -----
    out.push_str(&format!(
        "#set table(inset: (x: 0.8em, y: 0.55em), stroke: (x, y) => if y == 0 {{ none }} else {{ (top: 0.5pt + {rule}) }}, fill: (x, y) => if y == 0 {{ {primary} }} else if calc.even(y) {{ {zebra} }} else {{ none }})\n"
    ));
    out.push_str("#show table.cell.where(y: 0): set text(fill: white, weight: \"bold\")\n");

    // ---- Block code in a tinted, padded panel -----------------------------
    out.push_str(&format!(
        "#show raw.where(block: true): it => block(width: 100%, fill: {panel}, inset: (x: 1em, y: 0.8em), radius: 4pt, breakable: true)[#it]\n\n"
    ));

    // ---- Cover page -------------------------------------------------------
    out.push_str(&cover_page(a, logo_path, &primary, &accent, &muted, &rule));

    // ---- Optional index (table of contents) on its own page --------------
    if a.options.index {
        out.push_str(&index_block(a, &primary, &accent, &rule));
    }

    // ---- Body -------------------------------------------------------------
    out.push_str(body);
    out.push('\n');
    out
}

/// The cover page: a logo (if any), a generous top spacer, the oversized title,
/// an accent rule, and the brand header/footer text laid out as cover metadata.
/// The whole page suppresses the running header/footer via a page-local override,
/// then breaks so the body starts on a fresh page.
fn cover_page(
    a: &Assembled,
    logo_path: Option<&str>,
    primary: &str,
    accent: &str,
    muted: &str,
    rule: &str,
) -> String {
    let mut out = String::new();
    // The cover carries no running header/footer of its own.
    out.push_str("#page(header: none, footer: none)[\n");

    // Logo at the top of the cover.
    if let Some(path) = logo_path {
        out.push_str(&format!(
            "#v(0.6cm)\n#image({}, height: 1.7cm)\n",
            typst_string(path)
        ));
    }

    // A flexible spacer floats the title block down to roughly the upper third,
    // so the cover reads as composed rather than top-loaded. The 1.1fr / 1.9fr
    // split keeps it above centre.
    out.push_str("#v(1.1fr)\n");

    // A small accent eyebrow above the title gives the cover a designed feel even
    // when the brand sets no header text.
    let eyebrow = if a.brand.header_text.trim().is_empty() {
        "DOCUMENT".to_owned()
    } else {
        a.brand.header_text.trim().to_uppercase()
    };
    out.push_str(&format!(
        "#text(size: 9.5pt, weight: \"bold\", fill: {accent}, tracking: 0.18em)[#{}]\n#v(0.5cm)\n",
        typst_string(&eyebrow)
    ));

    // The title, oversized, with an accent rule beneath it. Justification and
    // hyphenation are forced off here so the display title never stretches words
    // or breaks mid-word the way justified body copy does.
    out.push_str(&format!(
        "#block(width: 85%)[#par(justify: false)[#text(size: 30pt, weight: \"bold\", fill: {primary}, hyphenate: false)[#{}]]]\n",
        typst_string(&a.title)
    ));
    out.push_str("#v(0.6cm)\n");
    out.push_str(&format!(
        "#box(width: 3.5cm, line(length: 100%, stroke: 3pt + {accent}))\n"
    ));

    out.push_str("#v(1.9fr)\n");

    // Cover metadata pinned near the bottom: a hairline and the brand footer text.
    out.push_str(&format!("#line(length: 100%, stroke: 0.5pt + {rule})\n#v(0.3cm)\n"));
    if !a.brand.footer_text.trim().is_empty() {
        out.push_str(&format!(
            "#text(size: 9pt, fill: {muted})[#{}]\n",
            typst_string(a.brand.footer_text.trim())
        ));
    }
    out.push_str("]\n#pagebreak(weak: true)\n\n");
    out
}

/// The running header (body pages): the brand header text in small muted caps with
/// a hairline rule beneath, or `none` when the brand left the header blank.
fn running_header(text: &str, muted: &str, rule: &str) -> String {
    if text.trim().is_empty() {
        return "none".to_owned();
    }
    format!(
        "[#text(size: 8pt, fill: {muted}, tracking: 0.08em)[#{}] #v(-0.4em) #line(length: 100%, stroke: 0.5pt + {rule})]",
        typst_string(text.trim())
    )
}

/// The running footer (body pages): a hairline rule above the brand footer text on
/// the left and a live `page / total` counter on the right. Returns `none` when
/// there is neither footer text nor page numbering.
fn running_footer(brand_text: &str, muted: &str, rule: &str, page_numbers: bool) -> String {
    let has_text = !brand_text.trim().is_empty();
    if !has_text && !page_numbers {
        return "none".to_owned();
    }
    let left = if has_text {
        format!(
            "text(size: 8pt, fill: {muted})[#{}]",
            typst_string(brand_text.trim())
        )
    } else {
        "[]".to_owned()
    };
    // `context` lets the counter read the resolved page/total at layout time.
    let right = if page_numbers {
        format!(
            "context text(size: 8pt, fill: {muted})[#counter(page).display(\"1 / 1\", both: true)]"
        )
    } else {
        "[]".to_owned()
    };
    format!(
        "[#line(length: 100%, stroke: 0.5pt + {rule}) #v(-0.2em) #grid(columns: (1fr, auto), {left}, {right})]"
    )
}

/// An index (table of contents) block on its own page listing each page's title in
/// render order, with a leader-dotted layout. Followed by a page break so the body
/// starts fresh. Every page the caller passes is a real page (a deliberately-blank
/// spacer included), so all are numbered.
fn index_block(a: &Assembled, primary: &str, accent: &str, rule: &str) -> String {
    let mut rows = String::new();
    for (i, _page) in a.pages.iter().enumerate() {
        // index · title · dotted leader filling the rest of the line.
        rows.push_str(&format!(
            "#grid(columns: (auto, auto, 1fr), gutter: 0.7em, align: (left, left, bottom), text(fill: {accent}, weight: \"bold\")[{}], text[#{}], box(width: 100%, inset: (bottom: 3pt))[#repeat(gap: 4pt)[#text(fill: {rule})[.]]])\n#v(0.4cm)\n",
            i + 1,
            typst_string(&a.page_label(i))
        ));
    }
    if rows.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    out.push_str(&format!(
        "#text(size: 20pt, weight: \"bold\", fill: {primary})[Contents]\n#v(0.3cm)\n"
    ));
    out.push_str(&format!("#line(length: 100%, stroke: 0.5pt + {rule})\n#v(0.7cm)\n"));
    out.push_str(&rows);
    out.push_str("#pagebreak(weak: true)\n\n");
    out
}

/// A Typst font-family list: the brand font (if any) first, then an embedded
/// fallback so an unknown brand font never breaks compilation.
fn font_list(brand: &str) -> String {
    let fallback = "\"Libertinus Serif\"";
    if brand.trim().is_empty() {
        format!("({fallback})")
    } else {
        format!("({}, {fallback})", typst_string(brand.trim()))
    }
}

/// A Typst color expression from a CSS-ish brand color string. Accepts `#rgb` /
/// `#rrggbb` hex (what Typst's `rgb` takes directly); anything else falls back
/// to `default` (already a valid Typst expression).
fn typst_color(value: &str, default: &str) -> String {
    let v = value.trim();
    let is_hex = v.starts_with('#')
        && matches!(v.len(), 4 | 7)
        && v[1..].chars().all(|c| c.is_ascii_hexdigit());
    if is_hex {
        format!("rgb({})", typst_string(v))
    } else {
        default.to_owned()
    }
}

/// The lowercase file extension Typst keys image-format detection off, derived
/// from a filename. Defaults to `png` when absent/unrecognized.
fn image_ext(filename: &str) -> String {
    let ext = filename
        .rsplit('.')
        .next()
        .filter(|e| !e.is_empty() && *e != filename)
        .unwrap_or("png")
        .to_ascii_lowercase();
    match ext.as_str() {
        "png" | "jpg" | "jpeg" | "gif" | "svg" | "webp" => ext,
        _ => "png".to_owned(),
    }
}

/// Flatten Typst diagnostics into a single human-readable message.
fn format_diags(diags: &[typst::diag::SourceDiagnostic]) -> String {
    diags
        .iter()
        .map(|d| d.message.to_string())
        .collect::<Vec<_>>()
        .join("; ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Brand, Colors, ImageAsset};

    #[test]
    fn renders_a_minimal_document_to_pdf_bytes() {
        let assembled = Assembled::new("Quote 001", "# Heading\n\nSome **bold** body text.");
        let pdf = render_pdf(&assembled).expect("render should succeed");
        assert!(pdf.starts_with(b"%PDF-"), "output is not a PDF");
        assert!(pdf.len() > 1000, "PDF unexpectedly small: {}", pdf.len());
    }

    #[test]
    fn multi_page_document_inserts_a_page_break_per_page() {
        // Two pages should compile to a PDF with two pages (each page-broken).
        let one = render_pdf(&Assembled::with_pages("Book", vec!["Only page.".to_owned()]))
            .expect("single page renders");
        let two = render_pdf(&Assembled::with_pages(
            "Book",
            vec!["First page.".to_owned(), "Second page.".to_owned()],
        ))
        .expect("two pages render");
        // The page count is encoded in the PDF; the two-page doc must report more
        // `/Page` objects than the one-page doc.
        let count = |pdf: &[u8]| {
            String::from_utf8_lossy(pdf).matches("/Type /Page\n").count()
                + String::from_utf8_lossy(pdf).matches("/Type/Page").count()
        };
        assert!(
            count(&two) > count(&one),
            "two-page doc should have more PDF pages than one-page doc",
        );
    }

    #[test]
    fn renders_with_a_brand_palette_and_header() {
        let brand = Brand {
            colors: Colors {
                primary: "#1d4ed8".into(),
                text: "#111827".into(),
                background: "#ffffff".into(),
                accent: "#db2777".into(),
                ..Colors::default()
            },
            header_text: "ACME Corp — Confidential".into(),
            footer_text: "Page footer".into(),
            ..Brand::default()
        };
        let assembled = Assembled::new("Branded", "Body with a [link](https://example.com).")
            .with_brand(brand);
        let pdf = render_pdf(&assembled).expect("branded render should succeed");
        assert!(pdf.starts_with(b"%PDF-"));
    }

    #[test]
    fn renders_with_a_logo_image() {
        // A tiny SVG exercises the image-loading path with no binary-format
        // fiddliness (Typst detects the format from the `.svg` extension).
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16"><rect width="16" height="16" fill="#1d4ed8"/></svg>"##.to_vec();
        let assembled =
            Assembled::new("With Logo", "Body.").with_logo(ImageAsset::new("", "logo.svg", svg));
        let pdf = render_pdf(&assembled).expect("logo render should succeed");
        assert!(pdf.starts_with(b"%PDF-"));
    }

    #[test]
    fn renders_with_page_numbers_and_index() {
        use crate::model::RenderOptions;
        let assembled = Assembled::with_pages(
            "Book",
            vec!["First page.".to_owned(), "Second page.".to_owned()],
        )
        .with_page_titles(vec!["Intro".to_owned(), "Details".to_owned()])
        .with_options(RenderOptions {
            page_numbers: true,
            index: true,
        });
        // The generated template must compile to a real PDF with both the page
        // counter (footer grid) and the index page present.
        let pdf = render_pdf(&assembled).expect("page-number + index render should succeed");
        assert!(pdf.starts_with(b"%PDF-"));
    }

    #[test]
    fn index_block_lists_every_page_including_blanks() {
        // A deliberately-blank spacer page is a real page, so it is numbered in
        // the index alongside the others (the caller already dropped any page that
        // shouldn't render).
        let assembled = Assembled::with_pages(
            "Book",
            vec!["One".to_owned(), "  ".to_owned(), "Three".to_owned()],
        )
        .with_page_titles(vec!["A".to_owned(), "Spacer".to_owned(), "C".to_owned()]);
        let block = index_block(
            &assembled,
            "rgb(\"#222\")",
            "rgb(\"#2563eb\")",
            "rgb(\"#ccc\")",
        );
        assert!(block.contains("\"A\""));
        assert!(block.contains("\"Spacer\""));
        assert!(block.contains("\"C\""));
        assert!(block.contains("#pagebreak"));
    }

    #[test]
    fn hex_color_helper_validates() {
        assert_eq!(typst_color("#abc", "X"), "rgb(\"#abc\")");
        assert_eq!(typst_color("#a1b2c3", "X"), "rgb(\"#a1b2c3\")");
        assert_eq!(typst_color("rebeccapurple", "X"), "X");
        assert_eq!(typst_color("", "X"), "X");
        assert_eq!(typst_color("#zzz", "X"), "X");
    }

    #[test]
    fn image_ext_is_normalized() {
        assert_eq!(image_ext("logo.PNG"), "png");
        assert_eq!(image_ext("a.jpeg"), "jpeg");
        assert_eq!(image_ext("noext"), "png");
        assert_eq!(image_ext("weird.bmp"), "png");
    }
}
