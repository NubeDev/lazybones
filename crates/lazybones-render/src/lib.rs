//! lazybones-render — pure markdown → Typst (PDF) and markdown → HTML rendering
//! for the document writer.
//!
//! The crate takes an **already-assembled** document — title, brand values,
//! resolved markdown (body + merged reference pages), and resolved logo/inline
//! image bytes — and has **no store dependency**, so it stays pure and
//! unit-testable. Assembly (resolving attached references into one markdown blob
//! and fetching image bytes from the `BlobStore`) lives in the API alongside the
//! export route, not here.
//!
//! - [`render_pdf`] builds a branded `.typ` (logo, brand colors, header/footer)
//!   and compiles it to PDF bytes via Typst.
//! - [`render_html`] turns the markdown into a brand-styled HTML page for the
//!   in-UI preview — the cheap, infallible fallback path.
//! - [`markdown_to_typst`] is the markdown → Typst markup converter, the crate's
//!   main implementation risk and the most heavily tested piece.
//!
//! # Phase 3a — de-risk spike (gate)
//!
//! The throwaway spike that first proved `typst::compile` → PDF-bytes against the
//! exact-pinned version set lives in [`mod@spike`] (test-only). The real
//! [`World`](world::RenderWorld) impl in [`world`] generalizes it to serve image
//! files too.
//!
//! ## Working pinned versions (verified by the spike + render tests)
//!
//! - `typst        = "=0.15.0"`
//! - `typst-pdf    = "=0.15.0"`
//! - `typst-assets = "=0.15.0"` (feature `fonts`, embedded font data)
//! - `typst-layout = "=0.15.0"` (home of `PagedDocument`; NOT re-exported by `typst`)
//! - `comemo       = "=0.5.1"`
//! - `pulldown-cmark = "0.13"`
//! - toolchain: Rust 1.96 (workspace `rust-version = 1.93`, edition 2024)

mod convert;
mod error;
mod html;
mod model;
mod pdf;
mod world;

pub use convert::{image_sources, markdown_to_typst, markdown_to_typst_plain};
pub use error::RenderError;
pub use html::render_html;
pub use model::{Assembled, Brand, Colors, Fonts, ImageAsset};
pub use pdf::render_pdf;

#[cfg(test)]
mod spike;
