//! Markdown → Typst markup conversion — the **main Phase 3b implementation
//! risk**, so it carries the bulk of the effort and the unit tests.
//!
//! The converter walks the [`pulldown_cmark`] event stream and emits Typst
//! markup. Two deliberate robustness choices keep arbitrary author markdown from
//! ever breaking the generated `.typ`:
//!
//! 1. **Every literal text run is emitted as a Typst string literal** (`#"..."`)
//!    rather than raw markup. A string used in content position renders as its
//!    characters with no markup interpretation, so author text containing `*`,
//!    `#`, `_`, `-`, `=`, `$`, leading list markers, … is impossible to
//!    mis-parse. Only `\` and `"` need escaping inside the string.
//! 2. **Structural elements use Typst's function forms** (`#heading`, `#list`,
//!    `#enum`, `#strong`, `#link`, `#table`, …) instead of line-start marker
//!    syntax, sidestepping every whitespace/line-start ambiguity.
//!
//! Inline images resolve through a caller-supplied closure that maps a markdown
//! `src` to the virtual path the Typst [`World`](crate::World) serves the bytes
//! at; an unresolved image degrades to its alt text.

use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag};

/// Convert `markdown` to Typst markup. `resolve_image` maps a markdown image
/// `src` to the virtual file path the [`World`](crate::World) serves its bytes
/// at; returning `None` drops the image to its alt text.
pub fn markdown_to_typst(markdown: &str, resolve_image: impl Fn(&str) -> Option<String>) -> String {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_TASKLISTS);

    let mut converter = Converter {
        stack: vec![Frame::new(Kind::Document)],
        resolve_image: &resolve_image,
    };
    for event in Parser::new_ext(markdown, options) {
        converter.event(event);
    }
    converter.finish()
}

/// Convenience for callers/tests with no inline images: every image degrades to
/// its alt text.
#[must_use]
pub fn markdown_to_typst_plain(markdown: &str) -> String {
    markdown_to_typst(markdown, |_| None)
}

/// Collect the `src` of every inline image (`![alt](src)`) in `markdown`, in
/// document order with duplicates removed. The API uses this to resolve image
/// bytes from the blob store before handing them to [`render_pdf`](crate::render_pdf),
/// keeping markdown parsing out of the API.
#[must_use]
pub fn image_sources(markdown: &str) -> Vec<String> {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_TASKLISTS);

    let mut srcs: Vec<String> = Vec::new();
    for event in Parser::new_ext(markdown, options) {
        if let Event::Start(Tag::Image { dest_url, .. }) = event {
            let src = dest_url.into_string();
            if !src.is_empty() && !srcs.contains(&src) {
                srcs.push(src);
            }
        }
    }
    srcs
}

/// What an open [`Frame`] represents — captured from the opening `Tag` so the
/// closing event never has to re-derive it from `TagEnd` payloads.
enum Kind {
    /// The root frame; its buffer is the final output.
    Document,
    Paragraph,
    Heading(u8),
    BlockQuote,
    CodeBlock { lang: Option<String> },
    /// An unordered (`None`) or ordered (`Some(start)`) list.
    List { start: Option<u64> },
    Item,
    Table,
    /// A table row; `header` distinguishes the head row (its cells are bolded).
    Row { header: bool },
    Cell,
    Emphasis,
    Strong,
    Strikethrough,
    Link { url: String },
    Image { url: String },
}

/// One open element: its kind, the markup accumulated for it, and the collected
/// child pieces (list items / row cells / table rows) that don't live inline.
struct Frame {
    kind: Kind,
    buf: String,
    /// List items, or the cells of the current table row.
    items: Vec<String>,
    /// Table rows, each flagged `header` and holding its cells.
    rows: Vec<(bool, Vec<String>)>,
}

impl Frame {
    fn new(kind: Kind) -> Self {
        Self {
            kind,
            buf: String::new(),
            items: Vec::new(),
            rows: Vec::new(),
        }
    }
}

struct Converter<'a> {
    stack: Vec<Frame>,
    resolve_image: &'a dyn Fn(&str) -> Option<String>,
}

impl Converter<'_> {
    /// Append literal markup to the innermost open frame.
    fn push(&mut self, s: &str) {
        self.stack
            .last_mut()
            .expect("the Document frame is never popped")
            .buf
            .push_str(s);
    }

    fn open(&mut self, kind: Kind) {
        self.stack.push(Frame::new(kind));
    }

    fn event(&mut self, event: Event<'_>) {
        match event {
            Event::Start(tag) => self.start(tag),
            Event::End(_) => self.end(),
            Event::Text(text) => self.text(&text),
            Event::Code(code) => {
                let raw = format!("#raw({})", typst_string(&code));
                self.push(&raw);
            }
            Event::SoftBreak => self.push(" "),
            Event::HardBreak => self.push("#linebreak()"),
            Event::Rule => self.push("#line(length: 100%)\n\n"),
            Event::TaskListMarker(done) => {
                self.push(if done { "☑ " } else { "☐ " });
            }
            // Raw HTML, math, and footnote references have no faithful Typst
            // markup here; drop them rather than emit something that could break
            // compilation. (Footnotes/math are a clean later enhancement.)
            Event::Html(_)
            | Event::InlineHtml(_)
            | Event::FootnoteReference(_)
            | Event::InlineMath(_)
            | Event::DisplayMath(_) => {}
        }
    }

    /// Emit a literal text run. In a code block the text is verbatim (collected
    /// for the block's `#raw` string); elsewhere it is a self-contained Typst
    /// string literal that can never be mis-parsed as markup.
    fn text(&mut self, text: &str) {
        if matches!(
            self.stack.last().map(|f| &f.kind),
            Some(Kind::CodeBlock { .. })
        ) {
            self.push(text);
        } else {
            // A `#`-prefixed string literal renders as its characters in content
            // position, with no markup interpretation.
            let lit = format!("#{}", typst_string(text));
            self.push(&lit);
        }
    }

    fn start(&mut self, tag: Tag<'_>) {
        match tag {
            Tag::Paragraph => self.open(Kind::Paragraph),
            Tag::Heading { level, .. } => self.open(Kind::Heading(heading_level(level))),
            Tag::BlockQuote(_) => self.open(Kind::BlockQuote),
            Tag::CodeBlock(kind) => {
                let lang = match kind {
                    CodeBlockKind::Fenced(info) => {
                        let lang = info.split_whitespace().next().unwrap_or("");
                        (!lang.is_empty()).then(|| lang.to_owned())
                    }
                    CodeBlockKind::Indented => None,
                };
                self.open(Kind::CodeBlock { lang });
            }
            Tag::List(start) => self.open(Kind::List { start }),
            Tag::Item => self.open(Kind::Item),
            Tag::Table(_) => self.open(Kind::Table),
            Tag::TableHead => self.open(Kind::Row { header: true }),
            Tag::TableRow => self.open(Kind::Row { header: false }),
            Tag::TableCell => self.open(Kind::Cell),
            Tag::Emphasis => self.open(Kind::Emphasis),
            Tag::Strong => self.open(Kind::Strong),
            Tag::Strikethrough => self.open(Kind::Strikethrough),
            Tag::Link { dest_url, .. } => self.open(Kind::Link {
                url: dest_url.into_string(),
            }),
            Tag::Image { dest_url, .. } => self.open(Kind::Image {
                url: dest_url.into_string(),
            }),
            // Everything else (HTML blocks, footnote/metadata blocks, definition
            // lists, super/subscript, …) is rendered through transparently: a
            // Document frame folds its inner content into the parent on close.
            _ => self.open(Kind::Document),
        }
    }

    fn end(&mut self) {
        let frame = self.stack.pop().expect("an End without a matching Start");
        match frame.kind {
            // The Document frame is the root and is only popped by `finish`.
            Kind::Document => {
                // A transparent container (HtmlBlock/FootnoteDefinition/Metadata)
                // opened a Document frame; fold its content into the parent.
                if self.stack.is_empty() {
                    self.stack.push(frame);
                } else {
                    self.push(&frame.buf);
                }
            }
            Kind::Paragraph => {
                let buf = frame.buf;
                self.push(&buf);
                self.push("\n\n");
            }
            Kind::Heading(level) => {
                let block = format!("#heading(level: {level})[{}]\n\n", frame.buf);
                self.push(&block);
            }
            Kind::BlockQuote => {
                let block = format!("#quote(block: true)[{}]\n\n", frame.buf.trim());
                self.push(&block);
            }
            Kind::CodeBlock { lang } => {
                let code = frame.buf.trim_end_matches('\n');
                let block = match lang {
                    Some(lang) => format!(
                        "#raw(block: true, lang: {}, {})\n\n",
                        typst_string(&lang),
                        typst_string(code)
                    ),
                    None => format!("#raw(block: true, {})\n\n", typst_string(code)),
                };
                self.push(&block);
            }
            Kind::List { start } => {
                let items: String = frame
                    .items
                    .iter()
                    .map(|item| format!("[{}]", item.trim()))
                    .collect::<Vec<_>>()
                    .join(", ");
                let block = match start {
                    Some(start) if start != 1 => format!("#enum(start: {start}, {items})\n\n"),
                    Some(_) => format!("#enum({items})\n\n"),
                    None => format!("#list({items})\n\n"),
                };
                self.push(&block);
            }
            Kind::Item => {
                // Hand the item's content up to its enclosing list frame.
                if let Some(parent) = self.stack.last_mut() {
                    parent.items.push(frame.buf);
                }
            }
            Kind::Cell => {
                if let Some(parent) = self.stack.last_mut() {
                    parent.items.push(frame.buf);
                }
            }
            Kind::Row { header } => {
                if let Some(parent) = self.stack.last_mut() {
                    parent.rows.push((header, frame.items));
                }
            }
            Kind::Table => {
                let cols = frame.rows.iter().map(|(_, c)| c.len()).max().unwrap_or(1);
                let mut cells = Vec::new();
                for (header, row) in &frame.rows {
                    for cell in row {
                        if *header {
                            cells.push(format!("[#strong[{}]]", cell.trim()));
                        } else {
                            cells.push(format!("[{}]", cell.trim()));
                        }
                    }
                }
                let block = format!(
                    "#table(columns: {cols}, {})\n\n",
                    cells.join(", ")
                );
                self.push(&block);
            }
            Kind::Emphasis => {
                let s = format!("#emph[{}]", frame.buf);
                self.push(&s);
            }
            Kind::Strong => {
                let s = format!("#strong[{}]", frame.buf);
                self.push(&s);
            }
            Kind::Strikethrough => {
                let s = format!("#strike[{}]", frame.buf);
                self.push(&s);
            }
            Kind::Link { url } => {
                let s = format!("#link({})[{}]", typst_string(&url), frame.buf);
                self.push(&s);
            }
            Kind::Image { url } => {
                let alt = frame.buf;
                match (self.resolve_image)(&url) {
                    Some(path) => {
                        let s = format!("#image({})", typst_string(&path));
                        self.push(&s);
                    }
                    // Unresolved image: fall back to its alt text run (already a
                    // `#`-prefixed string literal), or nothing if it had no alt.
                    None => self.push(&alt),
                }
            }
        }
    }

    fn finish(mut self) -> String {
        let root = self.stack.pop().expect("the Document frame");
        root.buf.trim().to_owned()
    }
}

/// Map a `pulldown-cmark` heading level to its `1..=6` depth.
fn heading_level(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

/// Render `s` as a Typst double-quoted string literal, escaping the only two
/// characters that are significant inside one (`\` and `"`) and normalizing
/// embedded control whitespace to spaces.
pub(crate) fn typst_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' | '\r' | '\t' => out.push(' '),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_paragraph_becomes_a_string_run() {
        let typ = markdown_to_typst_plain("Hello world");
        assert_eq!(typ, "#\"Hello world\"");
    }

    #[test]
    fn special_markdown_chars_cannot_break_markup() {
        // Asterisks, hashes, underscores, dollar signs and a leading list marker
        // all survive as literal text inside the string run.
        let typ = markdown_to_typst_plain(r"literal \*not bold\* and #hash and $5 and a/b");
        // The whole run is one string literal: no stray #strong / $ math / etc.
        assert!(typ.starts_with("#\""));
        assert!(!typ.contains("#strong"));
        assert!(typ.contains("not bold"));
        assert!(typ.contains("$5"));
    }

    #[test]
    fn headings_use_the_function_form_with_level() {
        let typ = markdown_to_typst_plain("# Title\n\n## Sub");
        assert!(typ.contains("#heading(level: 1)[#\"Title\"]"));
        assert!(typ.contains("#heading(level: 2)[#\"Sub\"]"));
    }

    #[test]
    fn emphasis_and_strong_and_strike_nest() {
        let typ = markdown_to_typst_plain("**bold _italic_** ~~gone~~");
        assert!(typ.contains("#strong[#\"bold \"#emph[#\"italic\"]]"));
        assert!(typ.contains("#strike[#\"gone\"]"));
    }

    #[test]
    fn inline_code_and_quotes_are_escaped() {
        let typ = markdown_to_typst_plain("use `cfg(\"x\")` here");
        // The backslash-escaped quote keeps the #raw string well-formed.
        assert!(typ.contains("#raw(\"cfg(\\\"x\\\")\")"));
    }

    #[test]
    fn fenced_code_block_keeps_language_and_body_verbatim() {
        let md = "```rust\nlet x = \"hi\";\n```";
        let typ = markdown_to_typst_plain(md);
        assert!(typ.contains("#raw(block: true, lang: \"rust\","));
        assert!(typ.contains("let x = \\\"hi\\\";"));
    }

    #[test]
    fn unordered_and_ordered_lists() {
        let bullets = markdown_to_typst_plain("- a\n- b");
        assert!(bullets.contains("#list([#\"a\"], [#\"b\"])"));

        let ordered = markdown_to_typst_plain("3. first\n4. second");
        assert!(ordered.contains("#enum(start: 3, [#\"first\"], [#\"second\"])"));
    }

    #[test]
    fn links_render_as_link_calls() {
        let typ = markdown_to_typst_plain("[lazy](https://example.com)");
        assert!(typ.contains("#link(\"https://example.com\")[#\"lazy\"]"));
    }

    #[test]
    fn blockquote_wraps_in_quote() {
        let typ = markdown_to_typst_plain("> quoted text");
        assert!(typ.contains("#quote(block: true)[#\"quoted text\"]"));
    }

    #[test]
    fn rule_becomes_a_line() {
        let typ = markdown_to_typst_plain("a\n\n---\n\nb");
        assert!(typ.contains("#line(length: 100%)"));
    }

    #[test]
    fn unresolved_image_degrades_to_alt_text() {
        let typ = markdown_to_typst_plain("![the alt](logo.png)");
        assert!(typ.contains("#\"the alt\""));
        assert!(!typ.contains("#image"));
    }

    #[test]
    fn resolved_image_emits_an_image_call_at_its_virtual_path() {
        let typ = markdown_to_typst("![alt](pic.png)", |src| {
            (src == "pic.png").then(|| "img-0.png".to_owned())
        });
        assert!(typ.contains("#image(\"img-0.png\")"));
    }

    #[test]
    fn table_renders_with_a_bold_header_row() {
        let md = "| A | B |\n|---|---|\n| 1 | 2 |";
        let typ = markdown_to_typst_plain(md);
        assert!(typ.contains("#table(columns: 2,"));
        assert!(typ.contains("[#strong[#\"A\"]]"));
        assert!(typ.contains("[#\"1\"]"));
    }

    #[test]
    fn empty_markdown_is_empty() {
        assert_eq!(markdown_to_typst_plain(""), "");
    }
}
