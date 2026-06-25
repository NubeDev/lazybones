//! Project the store's authoring records out to the file tree (the "before you
//! leave PC-1" half of content sync).

use std::path::Path;

use serde::Serialize;

use crate::asset::BlobStore;
use crate::error::{Result, StoreError};
use crate::handle::StoreHandle;

use super::{SyncReport, dirs, safe_stem};

/// Export every syncable record from `store` into a deterministic file tree under
/// `root` (created if absent). One YAML file per record, grouped by kind:
/// `documents/`, `skills/`, `tasks/`, `templates/`, `workflows/`, `assets/`.
/// Returns the per-kind counts.
///
/// Assets (the images documents reference) are exported as **both** their metadata
/// row (`assets/<id>.yaml`) **and** their bytes (`assets/blobs/<sha256>`), read
/// from `blobs` — so a synced document's images travel with it instead of
/// dangling on the other machine.
///
/// The output is byte-stable for an unchanged store, so a transport that diffs
/// (git) only sees real changes. This does **not** delete files for records that
/// no longer exist — pruning the tree is the transport's concern (a `git add -A`
/// stages deletions against the previous commit); export is purely additive per
/// run so a partial failure never drops data on disk.
///
/// # Errors
/// Returns [`StoreError::SyncIo`] if a directory or file write fails,
/// [`StoreError::SyncSerde`] if a record cannot be serialized, or the underlying
/// store error if a `list_*` read fails.
pub async fn export_all(
    store: &StoreHandle,
    blobs: &dyn BlobStore,
    root: &Path,
) -> Result<SyncReport> {
    Ok(SyncReport {
        documents: write_kind(root, dirs::DOCUMENTS, store.list_documents(None).await?)?,
        skills: write_kind(root, dirs::SKILLS, store.list_skills().await?)?,
        tasks: write_kind(root, dirs::TASKS, store.list_tasks(None).await?)?,
        templates: write_kind(root, dirs::TEMPLATES, store.list_templates().await?)?,
        workflows: write_kind(root, dirs::WORKFLOWS, store.list_runs().await?)?,
        assets: export_assets(store, blobs, root).await?,
    })
}

/// Export asset metadata rows **and** their blob bytes. The row goes to
/// `assets/<id>.yaml`; the bytes go to `assets/blobs/<sha256>` (content-addressed,
/// so identical images dedup on disk). An asset whose bytes are missing from the
/// blob store (a dangling row) is skipped rather than aborting the whole export.
async fn export_assets(
    store: &StoreHandle,
    blobs: &dyn BlobStore,
    root: &Path,
) -> Result<usize> {
    let assets = store.list_assets(None).await?;
    let dir = root.join(dirs::ASSETS);
    let blob_dir = dir.join(dirs::BLOBS_SUBDIR);
    std::fs::create_dir_all(&blob_dir)
        .map_err(|e| StoreError::SyncIo(format!("create {}: {e}", blob_dir.display())))?;

    let mut written = 0;
    for asset in &assets {
        // Read the bytes; a missing blob means a dangling row — skip it.
        let Ok(bytes) = blobs.get(&asset.sha256, asset.project.as_deref()).await else {
            continue;
        };
        let blob_path = blob_dir.join(safe_stem(&asset.sha256));
        std::fs::write(&blob_path, &bytes)
            .map_err(|e| StoreError::SyncIo(format!("write {}: {e}", blob_path.display())))?;
        write_record(&dir, &asset.id, asset)?;
        written += 1;
    }
    Ok(written)
}

/// Write every record of one kind into `<root>/<sub>/<id>.yaml`, creating the
/// sub-directory. Returns the count written. Generic over the kind so adding a
/// new one is a single line in [`export_all`].
fn write_kind<T: Identified + Serialize>(
    root: &Path,
    sub: &str,
    records: Vec<T>,
) -> Result<usize> {
    let dir = root.join(sub);
    std::fs::create_dir_all(&dir)
        .map_err(|e| StoreError::SyncIo(format!("create {}: {e}", dir.display())))?;
    for record in &records {
        write_record(&dir, record.sync_id(), record)?;
    }
    Ok(records.len())
}

/// Serialize `record` to `<dir>/<safe_stem(id)>.yaml`. Writes via a same-dir
/// temp file + rename so a crash mid-write never leaves a truncated record on
/// disk (the transport would otherwise commit a corrupt file).
fn write_record<T: Serialize>(dir: &Path, id: &str, record: &T) -> Result<()> {
    let yaml = serde_yaml::to_string(record)
        .map_err(|e| StoreError::SyncSerde(format!("serialize {id}: {e}")))?;
    let final_path = dir.join(format!("{}.yaml", safe_stem(id)));
    let tmp_path = dir.join(format!(".{}.yaml.tmp", safe_stem(id)));
    std::fs::write(&tmp_path, yaml)
        .map_err(|e| StoreError::SyncIo(format!("write {}: {e}", tmp_path.display())))?;
    std::fs::rename(&tmp_path, &final_path)
        .map_err(|e| StoreError::SyncIo(format!("rename {}: {e}", final_path.display())))?;
    Ok(())
}

/// A record that carries a string id and can be serialized — the only shape
/// `export_all` needs from each domain type.
pub(crate) trait Identified {
    fn sync_id(&self) -> &str;
}

impl Identified for crate::document::Document {
    fn sync_id(&self) -> &str {
        &self.id
    }
}
impl Identified for crate::skill::Skill {
    fn sync_id(&self) -> &str {
        &self.id
    }
}
impl Identified for crate::task::Task {
    fn sync_id(&self) -> &str {
        &self.id
    }
}
impl Identified for crate::template::Template {
    fn sync_id(&self) -> &str {
        &self.id
    }
}
impl Identified for crate::run::Run {
    fn sync_id(&self) -> &str {
        &self.id
    }
}
impl Identified for crate::asset::Asset {
    fn sync_id(&self) -> &str {
        &self.id
    }
}
