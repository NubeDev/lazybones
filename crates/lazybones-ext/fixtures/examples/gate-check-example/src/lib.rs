//! Example gate-check guest component.
//!
//! Implements the `lazybones:ext/gate-check` interface (see `../../../wit`). It is
//! intentionally tiny — the point is to prove the host plumbing (instantiation,
//! typed input/output, fuel/epoch/memory limits, trap isolation), not to be a
//! useful gate.
//!
//! Verdict policy:
//! - empty diff (`files-changed == 0`)            -> `skip`
//! - `task-summary` contains "fail"               -> `fail`
//! - otherwise                                    -> `pass`
//!
//! Plus one deliberate escape hatch for the host's resource-limit tests: a
//! `task-summary` of exactly `"runaway"` spins forever, so the host can assert the
//! fuel / epoch limiter kills it.

wit_bindgen::generate!({
    path: "../../../wit",
    world: "extension",
});

use exports::lazybones::ext::gate_check::{Guest, GateInput, Verdict, VerdictKind};

struct Component;

impl Guest for Component {
    fn run(input: GateInput) -> Verdict {
        // Deliberate runaway path for the host's fuel/epoch kill test. A real
        // guest would never do this; the host must survive it regardless.
        if input.task_summary == "runaway" {
            #[allow(clippy::empty_loop)]
            loop {
                core::hint::spin_loop();
            }
        }

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
