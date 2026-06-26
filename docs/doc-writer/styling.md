# Managing PDF & preview styling

How the document writer turns a document into a **branded PDF** (and a matching
**HTML preview**), and where to reach in to change the look. Read this before
touching the render layer — the two outputs are deliberately kept visually in
sync, so a change usually lands in two places.

## The big picture

```
Document (markdown pages) ─┐
Branding (colors/fonts/    ├─► API assembles an `Assembled` ─► lazybones-render ─┬─► render_pdf  → PDF bytes   (Typst)
  header/footer + logo)    │     (resolves references,       (pure, no store)    └─► render_html → HTML string (preview)
Logo / inline image bytes ─┘      fetches image bytes)
```

- **`crates/lazybones-render`** owns *all* visual styling. It is a pure crate:
  it takes an already-assembled document and produces bytes/markup. It has **no
  store/DB dependency**, so styling is unit-testable in isolation.
- **`render_pdf`** ([`pdf.rs`](../../crates/lazybones-render/src/pdf.rs)) builds a
  Typst template and compiles it. This is the **printed** output.
- **`render_html`** ([`html.rs`](../../crates/lazybones-render/src/html.rs)) emits
  a self-contained, brand-styled HTML page. This is the **in-UI preview** shown in
  an iframe while authoring.
- The two are **independent implementations of the same design** (Typst markup
  vs. CSS). There is no shared stylesheet — changing one does **not** change the
  other. **When you adjust the look, edit both** so the preview keeps matching the
  PDF.

## What the author controls (no code)

These reach styling through data, via the REST/MCP surface or the UI — no rebuild
needed.

| Surface | Field | Effect |
|---|---|---|
| **Branding** | `colors.primary` | Headings, title, table header fill, cover/heading ink |
| | `colors.accent` | Links, h3, the cover/heading accent rule, index numbers |
| | `colors.text` | Body text; muted chrome is derived from it |
| | `colors.background` | Page fill |
| | `colors.secondary` | Reserved (not yet mapped to a visual element) |
| | `fonts.heading` / `fonts.body` | Font families (see **Fonts** below) |
| | `header_text` | Running header on body pages + the uppercased cover **eyebrow** |
| | `footer_text` | Running footer (left) + cover metadata line |
| | `logo_asset_id` | Logo on the cover / preview lead |
| **Document** | `branding_id` | Which brand profile to render with (`None` → neutral default) |
| | `page_numbers` | `page / total` counter in the footer |
| | `index` | Prepend a "Contents" page listing each page title |

UI: brand profiles live in [`branding-page.tsx`](../../ui/src/features/branding/branding-page.tsx)
(colors, fonts, logo upload, header/footer); the per-document toggles
(`branding_id`, `page_numbers`, `index`) live in the document editor. The typed
API clients are [`branding.ts`](../../ui/src/lib/api/branding.ts) and
[`documents.ts`](../../ui/src/lib/api/documents.ts).

Colors must be `#rgb` or `#rrggbb` hex — anything else falls back to the neutral
default at render time (see `typst_color` in `pdf.rs` / `css_or` in `html.rs`).

## What a developer controls (code)

The *layout and rules* — cover composition, heading hierarchy, table/code/quote
styling, spacing, the running header/footer bands. All of it is in the two files
below.

### The PDF template — `pdf.rs`

`build_template()` is the entry point. It assembles a Typst source string in this
order:

1. **Page geometry + running header/footer** — `#set page(..)` with A4, margins,
   and the `running_header` / `running_footer` bands. The cover overrides these to
   `none` for itself.
2. **Text & paragraph defaults** — body font/size, `#set par(justify: true, ..)`,
   and a `#show raw` rule that puts code in DejaVu Sans Mono.
3. **Heading hierarchy** — `#show heading.where(level: N)` rules: h1 large with a
   short accent underline, h2 bold, h3 in the accent color.
4. **Links / quotes / lists** — accent underlined links, an accent-rule block
   quote, list indent/spacing.
5. **Tables** — `#set table(..)` with a filled header row (`primary`), zebra body
   rows, and soft hairline separators; header cells go white + bold.
6. **Code blocks** — `#show raw.where(block: true)` wraps block code in a tinted,
   padded, rounded panel.
7. **Cover page** — `cover_page()`: logo, eyebrow, oversized (non-justified)
   title, accent rule, bottom metadata band; then a page break.
8. **Index** — `index_block()`: a "Contents" page with dotted leaders, when
   enabled.
9. **Body** — the converted markdown, page-broken per document page.

Colors used by the template are derived once at the top of `build_template`:
`primary`, `accent`, `text_color`, `background`, plus computed tints —
`muted` (secondary ink), `panel` (code background), `zebra` (table stripe),
`rule` (hairlines). To retune the whole palette's *feel*, adjust those
`.lighten(..)` factors rather than every call site.

> **The one Typst gotcha — string vs. content position.** Author text is emitted
> as a Typst **string literal**. In *value* position you write `#"text"`; in
> *content* position (inside `[ ]`) a bare `"text"` renders the quote marks
> literally (smartquotes turn them into `“ ”`). So inside `[ ]` always use the
> `#`-prefixed form: `text(..)[#{typst_string(x)}]`. Every chrome helper
> (`cover_page`, `running_header`, `running_footer`, `index_block`) follows this.
> If you see stray curly quotes around the title/header/footer, this is why.

### The HTML preview — `html.rs`

`render_html()` builds a standalone page: each document page becomes one A4
`.doc-page` "sheet" on a grey desk; the cover (`.doc-cover` → logo, `.doc-eyebrow`,
`.doc-title`, `.doc-rule`) rides on the first sheet. The `<style>` block is a
format string with the brand `primary`/`accent`/`text`/`background` interpolated
in. The CSS intentionally mirrors `pdf.rs`:

- `thead th` filled with `primary` + white; `tbody tr:nth-child(even)` zebra;
  hairline row borders → matches the PDF table.
- `pre` tinted with `{primary}10` (8-digit hex alpha) → matches the code panel.
- `blockquote` with an accent left rule + muted italic → matches the quote.
- `.doc-eyebrow` uppercase/letter-spaced in accent; `.doc-rule` a short accent
  bar → matches the cover.

The logo and inline images are inlined as `data:` URIs (the preview iframe has no
base URL, so `/assets/<id>` paths would not resolve).

## Fonts — important constraint

The PDF compiles **offline** with only the fonts embedded in `typst-assets`
(see [`world.rs`](../../crates/lazybones-render/src/world.rs)). The available
families are **Libertinus Serif** (the default body/heading face), **DejaVu Sans
Mono** (code), and **New Computer Modern**. There is no bundled Inter/Helvetica.

`font_list()` puts the brand font first with **Libertinus Serif** as a fallback,
so an unknown `fonts.heading`/`fonts.body` never breaks compilation — but it also
means a brand font that isn't embedded simply won't appear in the PDF (it *will*
appear in the HTML preview if the viewer's browser has it, which is a place the
two outputs can legitimately diverge). To ship a new brand font in the PDF, add
its bytes to the render world's font set in `world.rs`.

## The markdown → Typst converter — `convert.rs`

Separate concern from *styling*: this turns each page's markdown into Typst markup
(headings, lists, tables, code, links, images). Two robustness rules: every text
run is a string literal (so author `*`, `#`, `$`, … can't break markup), and
structure uses Typst function forms (`#heading`, `#table`, …). One styling-relevant
detail: fenced code uses `typst_string_multiline()` (preserves `\n` as `\n`
escapes) rather than `typst_string()` (which collapses newlines to spaces) — that's
what keeps a multi-line code block from flowing into one line.

## How to change the look — workflow

1. Edit `pdf.rs` (and the matching CSS in `html.rs`).
2. `cargo test -p lazybones-render` — the tests assert the generated Typst/HTML
   compiles and contains the expected structure. Add/adjust an assertion for new
   chrome.
3. Rebuild + restart the daemon, then export and **look at it**:
   ```sh
   cargo build -p lazybones-cli --bin lazybonesd
   # restart lazybonesd (e.g. via `make dev`)
   curl -s "http://127.0.0.1:46787/documents/<id>/export.pdf" -o out.pdf
   ```
   A picture beats a diff: rasterize the PDF (e.g. `pdftoppm`/`pymupdf`) and eye
   the cover, a table, and a code block. The HTML preview is at
   `GET /documents/<id>/render`.
4. Confirm the preview still tracks the PDF — same brand colors should produce the
   same filled table header, code panel, accent rule, etc.

## Quick reference — where each element is styled

| Element | PDF (`pdf.rs`) | Preview (`html.rs`) |
|---|---|---|
| Cover (logo/eyebrow/title/rule) | `cover_page()` | `.doc-cover` markup + CSS |
| Running header | `running_header()` | `.page-foot` is footer; header is the cover eyebrow |
| Running footer + page numbers | `running_footer()` | `.page-foot` + `.page-num` |
| Contents / index page | `index_block()` | `index_html()` + `.doc-index` CSS |
| Headings h1/h2/h3 | `#show heading.where(..)` rules | `h1/h2/h3` CSS |
| Tables (header fill, zebra) | `#set table` + `#show table.cell` | `thead th` / `tbody tr` CSS |
| Code block panel | `#show raw.where(block: true)` | `pre` CSS |
| Block quote | `#show quote.where(block: true)` | `blockquote` CSS |
| Links | `#show link` | `a` CSS |
| Palette derivation | top of `build_template()` | `:root` vars + interpolation |
