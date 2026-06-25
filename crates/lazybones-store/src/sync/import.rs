//! Read the file tree back into the store (the "on boot, pull then catch up" half
//! of content sync).

use std::path::Path;

use serde::de::DeserializeOwned;

use crate::asset::{Asset, BlobStore};
use crate::document::Document;
use crate::error::{Result, StoreError};
use crate::handle::StoreHandle;
use crate::run::Run;
use crate::skill::Skill;
use crate::task::Task;
use crate::template::Template;

use super::{SyncReport, dirs, safe_stem};

/// Import every record from the file tree under `root` into `store`, idempotently.
/// Missing kind sub-directories are skipped, so a partial tree (only `skills/`,
/// say) imports cleanly and an empty/absent tree is a no-op. Returns per-kind
/// counts of records read.
///
/// Conflict policy is **last-writer-wins for authoring content** (documents,
/// skills, templates) and **spec-upsert for tasks** — the file overwrites the
/// stored record, preserving its `created_at`. Workflows (runs) carry live
/// lifecycle the local edge owns, so an existing run is left untouched and only a
/// missing one is created; this keeps the simple sync from clobbering runtime
/// state (the spec/status split the team-plane design rests on,
/// `docs/lazybones-server/`).
///
/// # Errors
/// Returns [`StoreError::SyncIo`] if the tree cannot be read,
/// [`StoreError::SyncSerde`] if a file is malformed, or the underlying store
/// error if a write fails.
pub async fn import_all(
    store: &StoreHandle,
    blobs: &dyn BlobStore,
    root: &Path,
) -> Result<SyncReport> {
    // Assets first, so a document imported below already has its images present.
    let mut report = SyncReport {
        assets: import_assets(store, blobs, root).await?,
        ..SyncReport::default()
    };

    for doc in read_kind::<Document>(root, dirs::DOCUMENTS)? {
        if store.get_document(&doc.id).await?.is_some() {
            store.update_document(&doc).await?;
        } else {
            store.create_document(&doc).await?;
        }
        report.documents += 1;
    }

    for skill in read_kind::<Skill>(root, dirs::SKILLS)? {
        if store.get_skill(&skill.id).await?.is_some() {
            store.update_skill(&skill).await?;
        } else {
            store.create_skill(&skill).await?;
        }
        report.skills += 1;
    }

    for task in read_kind::<Task>(root, dirs::TASKS)? {
        // `upsert_task` is the workfile-sync write: it lands the spec idempotently
        // without resurrecting lifecycle from the file.
        store.upsert_task(&task).await?;
        report.tasks += 1;
    }

    for template in read_kind::<Template>(root, dirs::TEMPLATES)? {
        if store.get_template(&template.id).await?.is_some() {
            store.update_template(&template).await?;
        } else {
            store.create_template(&template).await?;
        }
        report.templates += 1;
    }

    for run in read_kind::<Run>(root, dirs::WORKFLOWS)? {
        // Don't clobber a live run: the local edge owns workflow lifecycle.
        if store.get_run(&run.id).await?.is_none() {
            store.create_run(&run).await?;
        }
        report.workflows += 1;
    }

    Ok(report)
}

/// Import asset rows + their blob bytes. For each `assets/<id>.yaml`, read the
/// bytes from `assets/blobs/<sha256>`, write them into `blobs`, then create the
/// metadata row ([`create_asset`](crate::asset::create_asset) is
/// content-addressed, so a re-import dedups). An asset whose bytes are missing
/// from the tree is skipped — importing a row with no image would just dangle.
async fn import_assets(
    store: &StoreHandle,
    blobs: &dyn BlobStore,
    root: &Path,
) -> Result<usize> {
    let blob_dir = root.join(dirs::ASSETS).join(dirs::BLOBS_SUBDIR);
    let mut imported = 0;
    for asset in read_kind::<Asset>(root, dirs::ASSETS)? {
        let blob_path = blob_dir.join(safe_stem(&asset.sha256));
        let bytes = match std::fs::read(&blob_path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => {
                return Err(StoreError::SyncIo(format!(
                    "read {}: {e}",
                    blob_path.display()
                )));
            }
        };
        blobs
            .put(&asset.sha256, asset.project.as_deref(), &bytes)
            .await
            .map_err(|e| StoreError::SyncIo(format!("put blob {}: {e}", asset.sha256)))?;
        store.create_asset(&asset).await?;
        imported += 1;
    }
    Ok(imported)
}

/// Read and deserialize every `*.yaml` file directly under `<root>/<sub>`, sorted
/// by filename for a deterministic apply order. A missing directory yields an
/// empty vec (a partial tree is valid). Dot-files (the `.tmp` write-staging files
/// from [`export_all`](super::export_all)) and non-`.yaml` entries are ignored.
fn read_kind<T: DeserializeOwned>(root: &Path, sub: &str) -> Result<Vec<T>> {
    let dir = root.join(sub);
    let entries = match std::fs::read_dir(&dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(StoreError::SyncIo(format!("read dir {}: {e}", dir.display()))),
    };

    let mut paths: Vec<std::path::PathBuf> = entries
        .filter_map(std::result::Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.extension().is_some_and(|x| x == "yaml")
                && !p
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with('.'))
        })
        .collect();
    paths.sort();

    let mut out = Vec::with_capacity(paths.len());
    for path in paths {
        let text = std::fs::read_to_string(&path)
            .map_err(|e| StoreError::SyncIo(format!("read {}: {e}", path.display())))?;
        let value = serde_yaml::from_str(&text)
            .map_err(|e| StoreError::SyncSerde(format!("parse {}: {e}", path.display())))?;
        out.push(value);
    }
    Ok(out)
}
