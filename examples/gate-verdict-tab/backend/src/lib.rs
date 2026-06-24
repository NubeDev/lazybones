//! Backend gate-check guest for the `gate-verdict-tab` example extension.
//!
//! Implements the `lazybones:ext/gate-check` interface (see the host's
//! `crates/lazybones-ext/wit`). It is the same tiny guest as the
//! `gate-check-example` fixture — the point of this example is the **packaging**
//! (an embedded `lazybones.ext.toml` manifest + a federated UI tab), not a clever
//! gate.
//!
//! Verdict policy:
//! - empty diff (`files-changed == 0`)  -> `skip`
//! - `task-summary` contains "fail"     -> `fail`
//! - otherwise                          -> `pass`

wit_bindgen::generate!({
    path: "../../../crates/lazybones-ext/wit",
    world: "extension",
});

use exports::lazybones::ext::gate_check::{Guest, GateInput, Verdict, VerdictKind};

struct Component;

impl Guest for Component {
    fn run(input: GateInput) -> Verdict {
        if input.diff.files_changed == 0 {
            return Verdict {
                kind: VerdictKind::Skip,
                message: "no files changed; gate not applicable".to_string(),
            };
        }

        if input.task_summary.to_lowercase().contains("fail") {
            return Verdict {
                kind: VerdictKind::Fail,
                message: format!(
                    "gate failed: summary requested failure ({} files, +{} -{})",
                    input.diff.files_changed, input.diff.insertions, input.diff.deletions
                ),
            };
        }

        Verdict {
            kind: VerdictKind::Pass,
            message: format!(
                "ok: {} file(s) changed (+{} -{})",
                input.diff.files_changed, input.diff.insertions, input.diff.deletions
            ),
        }
    }
}

export!(Component);
