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
    // every document page lands on its own PDF page. Empty pages are dropped so a
    // blank page never produces a stray break.
    let body = assembled
        .pages
        .iter()
        .map(|page| markdown_to_typst(page, |src| resolve(src)))
        .filter(|typ| !typ.trim().is_empty())
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
fn build_template(a: &Assembled, logo_path: Option<&str>, body: &str) -> String {
    let c = &a.brand.colors;
    let f = &a.brand.fonts;

    let background = typst_color(&c.background, "white");
    let text_color = typst_color(&c.text, "rgb(\"#1a1a1a\")");
    let primary = typst_color(&c.primary, "rgb(\"#222222\")");
    let accent = typst_color(&c.accent, "rgb(\"#2563eb\")");
    let body_font = font_list(&f.body);
    let heading_font = font_list(&f.heading);

    let header = brand_band(&a.brand.header_text, &text_color);
    let footer = page_footer(&a.brand.footer_text, &text_color, a.options.page_numbers);

    let mut out = String::new();
    out.push_str(&format!(
        "#set page(paper: \"a4\", margin: (x: 2.2cm, top: 2.6cm, bottom: 2.4cm), fill: {background}, header: {header}, footer: {footer})\n"
    ));
    out.push_str(&format!(
        "#set text(font: {body_font}, fill: {text_color}, size: 11pt)\n"
    ));
    out.push_str(&format!(
        "#show heading: set text(font: {heading_font}, fill: {primary})\n"
    ));
    out.push_str(&format!("#show link: set text(fill: {accent})\n\n"));

    if let Some(path) = logo_path {
        out.push_str(&format!("#image({}, height: 1.4cm)\n", typst_string(path)));
        out.push_str("#v(0.3cm)\n");
    }

    out.push_str(&format!(
        "#text(size: 22pt, weight: \"bold\", fill: {primary})[{}]\n",
        typst_string(&a.title)
    ));
    out.push_str("#v(0.2cm)\n");
    out.push_str(&format!("#line(length: 100%, stroke: 0.6pt + {primary})\n"));
    out.push_str("#v(0.6cm)\n\n");

    if a.options.index {
        out.push_str(&index_block(a, &primary));
    }

    out.push_str(body);
    out.push('\n');
    out
}

/// The page footer: the brand footer band on the left and, when page numbers are
/// on, a live `page / total` counter on the right. Returns `none` when neither is
/// present so an unconfigured document keeps its empty footer.
fn page_footer(brand_text: &str, color: &str, page_numbers: bool) -> String {
    let band = brand_band(brand_text, color);
    if !page_numbers {
        return band;
    }
    // `context` lets the counter read the resolved page/total at layout time.
    let number = format!(
        "context [#text(size: 8.5pt, fill: {color})[#counter(page).display(\"1 / 1\", both: true)]]"
    );
    // Brand band (if any) on the left, page number pushed to the right.
    let left = if band == "none" { "[]".to_owned() } else { band };
    format!("[#grid(columns: (1fr, auto), {left}, {number})]")
}

/// An index (table of contents) block listing each non-empty page's title in
/// render order, followed by a page break so the body starts on a fresh page.
/// Mirrors the body's empty-page filtering so the numbering lines up.
fn index_block(a: &Assembled, primary: &str) -> String {
    let mut rows = String::new();
    let mut n = 0usize;
    for (i, page) in a.pages.iter().enumerate() {
        if page.trim().is_empty() {
            continue;
        }
        n += 1;
        rows.push_str(&format!(
            "#text(size: 11pt)[{}.#h(0.4em){}]\n#v(0.2cm)\n",
            n,
            typst_string(&a.page_label(i))
        ));
    }
    if rows.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    out.push_str(&format!(
        "#text(size: 15pt, weight: \"bold\", fill: {primary})[Index]\n#v(0.4cm)\n"
    ));
    out.push_str(&rows);
    out.push_str("#pagebreak()\n\n");
    out
}

/// A page header/footer band: small, muted text, or `none` when the brand left
/// it blank.
fn brand_band(text: &str, color: &str) -> String {
    if text.trim().is_empty() {
        "none".to_owned()
    } else {
        format!(
            "[#text(size: 8.5pt, fill: {color})[{}]]",
            typst_string(text)
        )
    }
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
    fn index_block_lists_non_empty_pages() {
        let assembled = Assembled::with_pages(
            "Book",
            vec!["One".to_owned(), "  ".to_owned(), "Three".to_owned()],
        )
        .with_page_titles(vec!["A".to_owned(), "Blank".to_owned(), "C".to_owned()]);
        let block = index_block(&assembled, "rgb(\"#222\")");
        // The blank page is skipped, and numbering renumbers around it.
        assert!(block.contains("\"A\""));
        assert!(block.contains("\"C\""));
        assert!(!block.contains("\"Blank\""));
        assert!(block.contains("#pagebreak()"));
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
