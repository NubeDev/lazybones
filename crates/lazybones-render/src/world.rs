//! A minimal in-memory Typst [`World`] — the offline compilation environment.
//!
//! Generalized from the Phase 3a spike's single-source world: it serves the
//! generated `main.typ` source plus any number of binary image files (logo +
//! inline images), and supplies the embedded `typst-assets` fonts. All paths are
//! virtual; nothing touches the real filesystem, keeping rendering pure.
//!
//! The 0.15 API specifics (font loading, `FileId`/`VirtualPath`, `LibraryExt`,
//! `today(Option<Duration>)`, `&LazyHash<_>` accessors) are the ones the spike
//! pinned and verified.

use std::collections::HashMap;

use typst::diag::{FileError, FileResult};
use typst::foundations::{Bytes, Datetime, Duration};
use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, LibraryExt, World};

/// An in-memory [`World`]: one `main.typ` source, a set of virtual image files,
/// and the embedded font set.
pub(crate) struct RenderWorld {
    library: LazyHash<Library>,
    book: LazyHash<FontBook>,
    fonts: Vec<Font>,
    main: Source,
    /// Virtual image files (logo + inline images), keyed by their `FileId`.
    files: HashMap<FileId, Bytes>,
}

impl RenderWorld {
    /// Build a world whose `main.typ` is `source`, serving each `(path, bytes)`
    /// in `files` as a virtual file the `.typ` can `image(...)`.
    pub(crate) fn new(source: &str, files: &[(String, Vec<u8>)]) -> Self {
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

        let main_id = FileId::new(RootedPath::new(
            VirtualRoot::Project,
            VirtualPath::new("main.typ").expect("valid virtual path"),
        ));
        let main = Source::new(main_id, source.to_owned());

        let mut file_map = HashMap::new();
        for (path, bytes) in files {
            let id = FileId::new(RootedPath::new(
                VirtualRoot::Project,
                VirtualPath::new(path).expect("valid virtual path"),
            ));
            file_map.insert(id, Bytes::new(bytes.clone()));
        }

        Self {
            library: LazyHash::new(Library::default()),
            book: LazyHash::new(book),
            fonts,
            main,
            files: file_map,
        }
    }
}

impl World for RenderWorld {
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
        self.files
            .get(&id)
            .cloned()
            .ok_or_else(|| FileError::NotFound(id.vpath().get_without_slash().into()))
    }

    fn font(&self, index: usize) -> Option<Font> {
        self.fonts.get(index).cloned()
    }

    fn today(&self, _offset: Option<Duration>) -> Option<Datetime> {
        None
    }
}
