//! Turn a [`Template`] into a concrete, pending [`Task`] under a workflow run.
//!
//! This is the recipe → instance step: it copies the template's spec text and
//! default tool onto a fresh `pending` task, records the provenance
//! (`template_id`) and the parent run (`run_id`), and carries the template's
//! `default_worktree_mode` into the task's `worktree_mode_override` so a
//! recipe intrinsically tied to a mode is honoured (otherwise the task inherits
//! the workspace mode — see the engine's `EffectiveGit` resolver).
//!
//! Seam for the deferred `Plan` layer: a Plan would call this once per task_def
//! to instantiate an ordered set as a whole.

use crate::task::Task;

use super::model::Template;

/// Instantiate `template` as a pending task `id` in workflow `run_id`.
///
/// `run` is the event-grouping label (today's `run` field); `run_id` is the
/// workflow FK. Deps/owns are wired by the caller (the API route), mirroring the
/// task authoring path. The returned task is `pending` with no claim state.
#[must_use]
pub fn instantiate(
    template: &Template,
    id: impl Into<String>,
    title: impl Into<String>,
    run: impl Into<String>,
    run_id: impl Into<String>,
    deps: Vec<String>,
) -> Task {
    let mut task = Task::seed(
        id,
        run,
        title,
        template.spec_template.clone(),
        deps,
        Vec::new(),
        template.default_tool.clone(),
    );
    task.run_id = Some(run_id.into());
    task.template_id = Some(template.id.clone());
    task.worktree_mode_override = template.default_worktree_mode;
    task
}
