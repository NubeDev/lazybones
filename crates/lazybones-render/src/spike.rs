//! Phase 3a de-risk spike — THROWAWAY.
//!
//! One test: compile a trivial `.typ` document to PDF bytes using an embedded
//! font supplied through a minimal, `typst-as-lib`-style [`World`] impl. If this
//! stops compiling/passing, the Typst version set is the thing to fix BEFORE the
//! real render layer (Phase 3b) is touched — this is the only gating phase.

use typst::diag::{FileError, FileResult};
use typst::foundations::{Bytes, Datetime, Duration};
use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, LibraryExt, World};
use typst_layout::PagedDocument;

/// Minimal in-memory [`World`]: one source file, fonts from `typst-assets`.
struct SpikeWorld {
    library: LazyHash<Library>,
    book: LazyHash<FontBook>,
    fonts: Vec<Font>,
    main: Source,
}

impl SpikeWorld {
    fn new(text: &str) -> Self {
        // Load every embedded face from typst-assets.
        let mut fonts = Vec::new();
        for data in typst_assets::fonts() {
            let bytes = Bytes::new(data);
            let mut index = 0;
            while let Some(font) = Font::new(bytes.clone(), index) {
                fonts.push(font);
                index += 1;
            }
        }
        let book = FontBook::from_fonts(&fonts);
        let vpath = VirtualPath::new("main.typ").expect("valid virtual path");
        let id = FileId::new(RootedPath::new(VirtualRoot::Project, vpath));
        let main = Source::new(id, text.to_string());
        Self {
            library: LazyHash::new(Library::default()),
            book: LazyHash::new(book),
            fonts,
            main,
        }
    }
}

impl World for SpikeWorld {
    fn library(&self) -> &LazyHash<Library> {
        &self.library
    }

    fn book(&self) -> &LazyHash<FontBook> {
        &self.book
    }

    fn main(&self) -> FileId {
        self.main.id()
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        if id == self.main.id() {
            Ok(self.main.clone())
        } else {
            Err(FileError::NotFound(id.vpath().get_without_slash().into()))
        }
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        Err(FileError::NotFound(id.vpath().get_without_slash().into()))
    }

    fn font(&self, index: usize) -> Option<Font> {
        self.fonts.get(index).cloned()
    }

    fn today(&self, _offset: Option<Duration>) -> Option<Datetime> {
        None
    }
}

#[test]
fn compiles_trivial_typ_to_pdf_bytes() {
    let world = SpikeWorld::new(
        "#set text(font: \"DejaVu Sans\")\n= Hello\nlazybones render spike.",
    );

    let result = typst::compile::<PagedDocument>(&world);
    let document = result
        .output
        .expect("typst::compile should produce a document");

    let pdf = typst_pdf::pdf(&document, &typst_pdf::PdfOptions::default())
        .expect("typst_pdf::pdf should produce PDF bytes");

    // A real PDF starts with the `%PDF-` magic and is non-trivially sized.
    assert!(pdf.starts_with(b"%PDF-"), "output is not a PDF");
    assert!(pdf.len() > 1000, "PDF unexpectedly small: {} bytes", pdf.len());
}
