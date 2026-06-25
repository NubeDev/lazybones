//! Content sync: a deterministic projection of the store's *authoring* records
//! to a flat file tree, and the idempotent import back.
//!
//! This is the lightweight counterpart to the cloud team plane
//! (`docs/lazybones-server/`): the heavy outbox/remote-SurrealDB machinery exists
//! to merge two planes writing *live runtime state* concurrently. The five nouns
//! here — documents, skills, tasks, templates, workflows — are file-shaped
//! authoring content with one writer at a time, so they need none of that. A
//! plain file tree, carried by any dumb transport (git, Google Drive — see
//! [`lazybones_gh::SyncRepo`]), is enough.
//!
//! ```text
//!   <root>/
//!     documents/<id>.yaml
//!     skills/<id>.yaml
//!     tasks/<id>.yaml
//!     templates/<id>.yaml
//!     workflows/<id>.yaml
//! ```
//!
//! Two properties make the tree safe to `git diff` and to sync last-writer-wins:
//!
//! - **Deterministic.** One record per file, named by id; fields serialize in
//!   struct-declaration order (YAML, matching the bundled `*.default.yaml`
//!   seeds). Re-exporting an unchanged store produces a byte-identical tree, so a
//!   commit only contains what actually changed.
//! - **Reuses the existing verbs.** [`export_all`] reads through the public
//!   `list_*` surface; [`import_all`] writes through `create`/`update`/`upsert`,
//!   so every store invariant (id-exists checks, `created_at` preservation,
//!   `upsert_task`'s spec-only write) is honoured. Nothing here talks to
//!   SurrealDB directly.
//!
//! The filename is cosmetic: the authoritative id always lives *inside* the YAML,
//! so an id containing a path separator is sanitised for the filename without
//! affecting round-trip correctness.

mod export;
mod import;

pub use export::export_all;
pub use import::import_all;

/// Per-kind counts from an [`export_all`] / [`import_all`] pass — what the caller
/// reports to the operator ("synced 4 skills, 2 documents, …").
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SyncReport {
    /// Documents exported/imported.
    pub documents: usize,
    /// Skills exported/imported.
    pub skills: usize,
    /// Tasks exported/imported.
    pub tasks: usize,
    /// Templates exported/imported.
    pub templates: usize,
    /// Workflows (runs) exported/imported.
    pub workflows: usize,
    /// Assets (images) exported/imported — metadata row + blob bytes.
    pub assets: usize,
}

impl SyncReport {
    /// The total number of records touched across all kinds.
    #[must_use]
    pub fn total(&self) -> usize {
        self.documents
            + self.skills
            + self.tasks
            + self.templates
            + self.workflows
            + self.assets
    }
}

/// The sub-directory name each kind lives under in the export tree.
pub(crate) mod dirs {
    pub(crate) const DOCUMENTS: &str = "documents";
    pub(crate) const SKILLS: &str = "skills";
    pub(crate) const TASKS: &str = "tasks";
    pub(crate) const TEMPLATES: &str = "templates";
    pub(crate) const WORKFLOWS: &str = "workflows";
    pub(crate) const ASSETS: &str = "assets";
    /// Sub-directory of `assets/` holding the raw blob bytes, keyed by sha256.
    pub(crate) const BLOBS_SUBDIR: &str = "blobs";
}

/// Turn a record id into a safe, stable filename stem. Ids are friendly
/// kebab-case in practice, but a stray `/` (or `..`) would escape the kind dir,
/// so map any path-significant character to `_`. The id inside the file stays
/// authoritative, so this never has to be reversed.
pub(crate) fn safe_stem(id: &str) -> String {
    id.chars()
        .map(|c| if c == '/' || c == '\\' || c == ':' { '_' } else { c })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::StoreHandle;
    use crate::asset::{Asset, FileBlobStore, sha256_hex};
    use crate::connect::StoreEngine;
    use crate::skill::Skill;
    use crate::template::Template;

    async fn store() -> StoreHandle {
        StoreHandle::open(&StoreEngine::Memory, "lazybones", "test", "test-key")
            .await
            .unwrap()
    }

    /// A file blob store rooted in a fresh tempdir (the daemon's data dir stand-in).
    fn blobs(dir: &std::path::Path) -> FileBlobStore {
        FileBlobStore::new(dir.join("data"))
    }

    #[tokio::test]
    async fn export_then_import_into_a_fresh_store_round_trips() {
        let src = store().await;
        let now = "2026-01-01T00:00:00Z";
        src.create_skill(&Skill::new("review-rust", "Review", "d", "body", now))
            .await
            .unwrap();
        src.create_template(&Template::new(
            "ship-it",
            "Ship it",
            "a deploy template",
            "spec body",
            None,
            None,
            None,
            None,
            now,
        ))
        .await
        .unwrap();

        let tmp = tempfile::tempdir().unwrap();
        let src_blobs = blobs(tmp.path());
        let out = export_all(&src, &src_blobs, tmp.path()).await.unwrap();
        assert_eq!(out.skills, 1);
        assert_eq!(out.templates, 1);

        // A second machine: empty store + empty blob store, same tree.
        let dst_dir = tempfile::tempdir().unwrap();
        let dst = store().await;
        let dst_blobs = blobs(dst_dir.path());
        let imported = import_all(&dst, &dst_blobs, tmp.path()).await.unwrap();
        assert_eq!(imported.skills, 1);
        assert_eq!(imported.templates, 1);

        let skill = dst.get_skill("review-rust").await.unwrap().unwrap();
        assert_eq!(skill.title, "Review");
        assert_eq!(skill.body, "body");
        assert!(dst.get_template("ship-it").await.unwrap().is_some());
    }

    #[tokio::test]
    async fn document_images_travel_with_the_sync() {
        use crate::asset::BlobStore;
        let now = "2026-01-01T00:00:00Z";
        let src = store().await;
        let tmp = tempfile::tempdir().unwrap();
        let src_blobs = blobs(tmp.path());

        // An "image": some bytes in the blob store + its metadata row.
        let png = b"\x89PNG fake image bytes".to_vec();
        let sha = sha256_hex(&png);
        src_blobs.put(&sha, None, &png).await.unwrap();
        src.create_asset(&Asset::new("logo-1", "logo.png", "image/png", png.len() as u64, &sha, now))
            .await
            .unwrap();

        let out = export_all(&src, &src_blobs, tmp.path()).await.unwrap();
        assert_eq!(out.assets, 1);
        // Both the row and the bytes are on disk.
        assert!(tmp.path().join("assets/logo-1.yaml").exists());
        assert!(tmp.path().join(format!("assets/blobs/{sha}")).exists());

        // Second machine: the asset row AND the bytes are restored.
        let dst_dir = tempfile::tempdir().unwrap();
        let dst = store().await;
        let dst_blobs = blobs(dst_dir.path());
        let imported = import_all(&dst, &dst_blobs, tmp.path()).await.unwrap();
        assert_eq!(imported.assets, 1);

        let got = dst.get_asset("logo-1").await.unwrap().unwrap();
        assert_eq!(got.sha256, sha);
        assert_eq!(dst_blobs.get(&sha, None).await.unwrap(), png);
    }

    #[tokio::test]
    async fn export_is_byte_deterministic() {
        let src = store().await;
        src.create_skill(&Skill::new(
            "a",
            "A",
            "d",
            "b",
            "2026-01-01T00:00:00Z",
        ))
        .await
        .unwrap();

        let t1 = tempfile::tempdir().unwrap();
        let t2 = tempfile::tempdir().unwrap();
        export_all(&src, &blobs(t1.path()), t1.path()).await.unwrap();
        export_all(&src, &blobs(t2.path()), t2.path()).await.unwrap();

        let f1 = std::fs::read(t1.path().join("skills/a.yaml")).unwrap();
        let f2 = std::fs::read(t2.path().join("skills/a.yaml")).unwrap();
        assert_eq!(f1, f2, "re-export of an unchanged store must be identical");
    }

    #[tokio::test]
    async fn import_updates_existing_records_last_writer_wins() {
        let src = store().await;
        src.create_skill(&Skill::new(
            "x",
            "Original",
            "d",
            "b",
            "2026-01-01T00:00:00Z",
        ))
        .await
        .unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let b = blobs(tmp.path());
        export_all(&src, &b, tmp.path()).await.unwrap();

        // Hand-edit the exported file (what another machine's commit looks like).
        let path = tmp.path().join("skills/x.yaml");
        let body = std::fs::read_to_string(&path).unwrap();
        std::fs::write(&path, body.replace("Original", "Edited")).unwrap();

        // Import over the existing record: the edit wins, create_at is preserved.
        import_all(&src, &b, tmp.path()).await.unwrap();
        let got = src.get_skill("x").await.unwrap().unwrap();
        assert_eq!(got.title, "Edited");
        assert_eq!(got.created_at, "2026-01-01T00:00:00Z");
    }

    #[tokio::test]
    async fn import_of_a_missing_or_empty_tree_is_a_noop() {
        let dst = store().await;
        let tmp = tempfile::tempdir().unwrap();
        // No sub-dirs at all.
        let report = import_all(&dst, &blobs(tmp.path()), tmp.path()).await.unwrap();
        assert_eq!(report.total(), 0);
    }
}
