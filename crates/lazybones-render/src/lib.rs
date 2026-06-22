//! lazybones-render — pure Typst → PDF rendering for the document writer.
//!
//! # Phase 3a — de-risk spike (gate)
//!
//! This crate currently contains ONLY the throwaway spike that proves the
//! `typst::compile` → PDF-bytes path works against a known-good, exact-pinned
//! version set with an embedded font. The Typst API churns between releases, so
//! Phase 3a locks the versions before the real markdown→Typst converter
//! (`render_pdf` / `render_html`, Phase 3b) is built on top.
//!
//! ## Working pinned versions (verified by the spike test below)
//!
//! - `typst        = "=0.15.0"`
//! - `typst-pdf    = "=0.15.0"`
//! - `typst-assets = "=0.15.0"` (feature `fonts`, embedded font data)
//! - `typst-layout = "=0.15.0"` (home of `PagedDocument`; NOT re-exported by `typst`)
//! - `comemo       = "=0.5.1"`
//! - toolchain: Rust 1.96 (workspace `rust-version = 1.93`, edition 2024)
//!
//! ## Key 0.15 API notes for the next task (3b)
//!
//! These were reconciled against the real 0.15.0 source — the API differs from
//! earlier Typst releases, so follow these exactly:
//!
//! - `typst::compile::<PagedDocument>(&world)` returns
//!   [`typst::diag::Warned<SourceResult<PagedDocument>>`]; read `.output`.
//!   `PagedDocument` comes from the `typst_layout` crate (a direct dep) — it is
//!   NOT under `typst::layout` and is NOT re-exported by `typst`.
//! - `typst_pdf::pdf(&document, &PdfOptions::default())` → `SourceResult<Vec<u8>>`.
//! - Fonts: iterate `typst_assets::fonts()` (each is `&'static [u8]`), wrap with
//!   `Bytes::new`, load faces via `Font::new(bytes, index)`, build the
//!   `FontBook` with `FontBook::from_fonts`.
//! - `FileId::new(RootedPath::new(VirtualRoot::Project, VirtualPath::new(path)?))`
//!   — `FileId::new` now takes a single `RootedPath`; `VirtualPath::new` returns
//!   a `Result`. All three live under `typst::syntax`.
//! - `Library::default()` requires the `typst::LibraryExt` trait in scope.
//! - `World::today(&self, offset: Option<typst::foundations::Duration>)` — the
//!   offset is a `Duration`, not an `i64`.
//! - The `World` impl must hand back `library`/`book` as `&LazyHash<_>`.

#[cfg(test)]
mod spike;
