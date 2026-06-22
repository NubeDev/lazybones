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

    let body = markdown_to_typst(&assembled.markdown, |src| {
        image_paths
            .iter()
            .find(|(s, _)| s == src)
            .map(|(_, path)| path.clone())
    });

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
    let footer = brand_band(&a.brand.footer_text, &text_color);

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

    out.push_str(body);
    out.push('\n');
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
